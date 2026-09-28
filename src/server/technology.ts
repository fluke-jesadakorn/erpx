import "server-only";
import { createHash, randomUUID } from "node:crypto";
import type { PoolClient } from "pg";
import {
  provisioningInput,
  type ProvisioningInput,
  type ProvisioningPreview,
  type TechnologyData,
} from "@/lib/technology";
import {
  packageDefinition,
  type AccessPackageDefinition,
} from "@/lib/people-lifecycle";
import {
  directoryChangeInput,
  type DirectoryChangeInput,
  type DirectoryInventory,
} from "@/lib/directory-operations";
import type { PostgresDatabase } from "./runtime/postgres";
import { iamDb } from "./iam-db";
import { assertCompanyScope, type IamActor } from "./iam-context";
import { requirePermission } from "./iam-permissions";
import { iamCommand } from "./iam-transactions";
import { requireFreshMfa } from "./iam-mfa";
import {
  grantInput,
  validateIamGrant,
  insertIamGrant,
  proposeIamReview,
} from "./iam-service";
import { prepareDirectoryPreview } from "./directory-operations";
import { ApiError } from "./http";
const digest = (v: unknown) =>
  createHash("sha256").update(JSON.stringify(v)).digest("hex");
function fail(
  message: string,
  code = "PROVISIONING_CHANGED",
  status = 409,
): never {
  throw new ApiError(status, code, message);
}
function scope(actor: IamActor, permission = "iam.directory.apply") {
  assertCompanyScope(actor);
  if (actor.platform)
    fail("Use a Company workspace.", "COMPANY_SCOPE_REQUIRED", 403);
  requirePermission(actor, permission);
}
export async function technologyData(
  raw: PostgresDatabase,
  actor: IamActor,
): Promise<TechnologyData> {
  scope(actor, "iam.read");
  const db = iamDb(raw);
  const applications = (
    await db.pool.query(
      "SELECT ap.*,people_application_active(ap.id,$2) active FROM people_applied_packages ap WHERE company_id=$1 ORDER BY created_at DESC",
      [actor.companyId, Date.now()],
    )
  ).rows;
  const operations = (
    await db.pool.query(
      "SELECT op.id,op.status,op.version,op.last_error,op.steps,op.created_at,(SELECT count(*)::int FROM people_provision_grants g JOIN iam_reviews r ON r.id=g.review_id WHERE g.operation_id=op.id AND r.status='pending') AS \"reviewsPending\" FROM people_provision_operations op WHERE company_id=$1 ORDER BY created_at DESC LIMIT 200",
      [actor.companyId],
    )
  ).rows;
  return {
    applications: applications.map((a) => ({
      ...a,
      scope: typeof a.scope === "string" ? JSON.parse(a.scope) : a.scope,
    })),
    operations,
  };
}
type PersonPlan = {
  id: string;
  personId: string;
  name: string;
  positionId: string | null;
  branchId: string | null;
  departmentId: string | null;
  engagementVersion: number;
  packageId: string;
  revision: number;
  definition: AccessPackageDefinition;
  oldIds: string[];
  users: string[];
  directoryMembers: string[];
  retained: string[];
};
async function buildPreview(
  c: PoolClient,
  actor: IamActor,
  input: ProvisioningInput,
) {
  const parent = (
    await c.query("SELECT valid_until FROM assignments WHERE id=$1", [
      actor.assignmentId,
    ])
  ).rows[0];
  if (
    !parent ||
    (!input.revokeOnly &&
      (input.validUntil <= Date.now() ||
        input.validUntil > Number(parent.valid_until)))
  )
    fail("Choose an expiry within your delegated authority.");
  const plans: PersonPlan[] = [],
    changes: DirectoryChangeInput[] = [],
    inventories = new Map<
      string,
      { version: number; inventory: DirectoryInventory }
    >();
  const beforeApps = (
    await c.query(
      "SELECT ap.* FROM people_applied_packages ap WHERE company_id=$1 AND ended_at IS NULL ORDER BY id",
      [actor.companyId],
    )
  ).rows;
  const sources = (
    await c.query(
      "SELECT s.* FROM people_directory_membership_sources s JOIN iam_connections c ON c.id=s.connection_id WHERE c.company_id=$1 ORDER BY s.connection_id,s.group_id,s.member_id,s.source_id",
      [actor.companyId],
    )
  ).rows;
  for (const engagementId of input.engagementIds) {
    const e = (
      await c.query(
        "SELECT e.*,p.legal_name,p.user_id,p.status person_status FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.id=$1 AND e.company_id=$2 AND p.merged_into_id IS NULL",
        [engagementId, actor.companyId],
      )
    ).rows[0];
    if (
      !e ||
      (!input.revokeOnly &&
        (e.status !== "active" || e.person_status !== "active"))
    )
      fail(
        "Complete HR placement before applying employee resources.",
        "PLACEMENT_REQUIRED",
      );
    const cutoff = (
      await c.query(
        "SELECT 1 FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3",
        [actor.companyId, e.person_id, Date.now()],
      )
    ).rowCount;
    if (cutoff && !input.revokeOnly) {
      if (
        !input.restoreAccess ||
        !(
          await c.query(
            "SELECT 1 FROM people_lifecycle_cases c WHERE c.company_id=$1 AND c.engagement_id=$2 AND c.status='completed' AND c.workflow='placement' AND c.kind IN ('onboarding','transfer') AND c.effective_at>(SELECT max(effective_at) FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$3 AND cancelled_at IS NULL AND restored_at IS NULL)",
            [actor.companyId, e.id, e.person_id],
          )
        ).rowCount
      )
        fail(
          "Confirm restoration after HR completes a new placement. Old grants will be revoked permanently.",
          "ACCESS_CUTOFF_ACTIVE",
        );
    }
    const binding = input.packageId
      ? { package_id: input.packageId, revision: input.revision }
      : (
          await c.query(
            "SELECT package_id,revision FROM people_position_packages WHERE company_id=$1 AND position_id=$2",
            [actor.companyId, e.position_id],
          )
        ).rows[0];
    if (!binding?.revision)
      fail(
        "Publish and bind an access package, or select a published package.",
        "PACKAGE_REQUIRED",
      );
    const revision = (
      await c.query(
        "SELECT r.definition FROM people_package_revisions r WHERE r.company_id=$1 AND r.package_id=$2 AND r.revision=$3",
        [actor.companyId, binding.package_id, binding.revision],
      )
    ).rows[0];
    if (!revision) fail("Choose a published package revision.");
    const recipe = packageDefinition.parse(revision.definition);
    let definition = {
      ...recipe,
      grants: recipe.grants.filter(
        (g) => !input.exclude.includes("role:" + g.roleId),
      ),
      groupIds: recipe.groupIds.filter(
        (id) => !input.exclude.includes("group:" + id),
      ),
      directoryGroupIds: recipe.directoryGroupIds.filter(
        (id) => !input.exclude.includes("directory:" + id),
      ),
      windowsPolicies: recipe.windowsPolicies.filter(
        (p) => !input.exclude.includes("gpo:" + p.id),
      ),
    };
    if (input.revokeOnly)
      definition = {
        ...definition,
        grants: [],
        groupIds: [],
        directoryGroupIds: [],
        windowsPolicies: [],
        tasks: [],
      };
    for (const groupId of definition.groupIds) {
      if (
        !(
          await c.query(
            "SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2 AND NOT (definition->>'archived')::boolean AND um_person_in_unit($1,$3,definition->>'scopeNodeId')",
            [actor.companyId, groupId, e.person_id],
          )
        ).rowCount
      )
        fail("An ERPX group is unavailable or outside this employee's scope.");
    }
    const users = (
      await c.query(
        "SELECT DISTINCT u.user_id FROM (SELECT user_id FROM hr_people WHERE company_id=$1 AND id=$2 UNION SELECT user_id FROM iam_members WHERE company_id=$1 AND person_id=$2) u LEFT JOIN iam_members m ON m.company_id=$1 AND m.user_id=u.user_id WHERE u.user_id IS NOT NULL AND COALESCE(m.status,'active')='active' AND (CASE WHEN $3='local' THEN COALESCE(m.login_policy,'local')!='directory' ELSE m.login_policy='directory' AND m.connection_id=$4 END) ORDER BY u.user_id",
        [
          actor.companyId,
          e.person_id,
          definition.account,
          definition.connectionId,
        ],
      )
    ).rows.map((r) => String(r.user_id));
    if (definition.grants.length && !users.length)
      fail(
        "Verify and link the account before requesting ERPX roles.",
        "ACCOUNT_REQUIRED",
      );
    for (const userId of users)
      for (const grant of definition.grants) {
        const requested = grantInput.parse({
          ...grant,
          userId,
          validUntil: Math.min(grant.validUntil, input.validUntil),
          reason: input.reason,
        });
        if (requested.validUntil <= Date.now())
          fail(
            "The package contains an expired role grant. Publish an updated revision.",
          );
        const role = (
          await c.query(
            "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active' AND published_revision=$3",
            [grant.roleId, actor.companyId, grant.expectedRoleRevision],
          )
        ).rowCount;
        if (!role)
          fail(
            "The requested role revision changed. Update the package first.",
          );
      }
    const old = input.revokeOnly
      ? beforeApps.filter(
          (a) =>
            a.person_id === e.person_id &&
            input.revokeApplicationIds.includes(a.id),
        )
      : input.sourceKind === "position"
        ? beforeApps.filter(
            (a) => a.person_id === e.person_id && a.source_kind === "position",
          )
        : [];
    if (input.revokeOnly && !old.length)
      fail("The selected applied source changed.");
    const retained = beforeApps
      .filter((a) => a.person_id === e.person_id && !old.includes(a))
      .flatMap((a) => [a.definition.name + " · " + a.source_kind]);
    const directoryMembers: string[] = [];
    const connectionIds = [
      ...new Set(
        [
          definition.connectionId,
          ...old.map((a) => a.definition.connectionId),
        ].filter((v): v is string => !!v),
      ),
    ];
    for (const connectionId of connectionIds) {
      let state = inventories.get(connectionId);
      if (!state) {
        const row = (
          await c.query(
            "SELECT c.version,s.inventory,s.write_enabled,s.tested_version,c.status FROM iam_connections c JOIN people_directory_state s ON s.connection_id=c.id WHERE c.id=$1 AND c.company_id=$2",
            [connectionId, actor.companyId],
          )
        ).rows[0];
        if (
          !row?.inventory ||
          row.status !== "active" ||
          !row.write_enabled ||
          row.tested_version !== row.version
        )
          fail(
            "Refresh and enable this connection before provisioning.",
            "DIRECTORY_READ_ONLY",
          );
        state = { version: row.version, inventory: row.inventory };
        inventories.set(connectionId, state);
      }
      const members = (
        await c.query(
          "SELECT DISTINCT i.external_id FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE i.connection_id=$1 AND m.company_id=$2 AND m.person_id=$3 AND m.status='active' ORDER BY i.external_id",
          [connectionId, actor.companyId, e.person_id],
        )
      ).rows.map((r) => String(r.external_id));
      const wanted =
        definition.connectionId === connectionId
          ? definition.directoryGroupIds
          : [];
      if (wanted.length && !members.length)
        fail(
          "Verify and link the directory account first.",
          "ACCOUNT_REQUIRED",
        );
      if (connectionId === definition.connectionId)
        directoryMembers.push(...members);
      for (const groupId of wanted) {
        const group = state.inventory.objects.find(
          (o) => o.id === groupId && o.kind === "group",
        );
        if (!group) fail("Refresh the package's directory groups.");
        const missing = members.filter((id) => !group.memberIds.includes(id));
        if (missing.length)
          changes.push(
            directoryChangeInput.parse({
              connectionId,
              expectedVersion: state.version,
              action: {
                kind: "group.members",
                objectId: groupId,
                memberIds: missing,
                action: "add",
              },
              reason: input.reason,
            }),
          );
      }
      for (const groupId of [
        ...new Set(
          old
            .filter((a) => a.definition.connectionId === connectionId)
            .flatMap((a) => a.definition.directoryGroupIds as string[]),
        ),
      ]) {
        if (wanted.includes(groupId)) continue;
        // Retire every replaced package source under the Directory worker's
        // connection lock, including memberships another source still needs.
        const ownedMembers = members.filter(
          (memberId) =>
            sources.some(
              (s) =>
                s.connection_id === connectionId &&
                s.group_id === groupId &&
                s.member_id === memberId &&
                old.some((a) => a.id === s.application_id),
            ),
        );
        const removable = ownedMembers.filter(
          (memberId) =>
            !sources.some(
              (s) =>
                s.connection_id === connectionId &&
                s.group_id === groupId &&
                s.member_id === memberId &&
                !old.some((a) => a.id === s.application_id),
            ),
        );
        if (ownedMembers.length)
          changes.push(
            directoryChangeInput.parse({
              connectionId,
              expectedVersion: state.version,
              action: {
                kind: "group.members",
                objectId: groupId,
                memberIds: ownedMembers,
                action: "remove",
              },
              reason: input.reason,
            }),
          );
        if (members.some((id) => !removable.includes(id)))
          retained.push(
            "Directory membership from another or untracked source · " +
              groupId,
          );
      }
    }
    plans.push({
      id: e.id,
      personId: e.person_id,
      name: e.legal_name,
      positionId: e.position_id,
      branchId: e.branch_id,
      departmentId: e.department_id,
      engagementVersion: e.version,
      packageId: binding.package_id,
      revision: binding.revision,
      definition,
      oldIds: old.map((a) => a.id),
      users,
      directoryMembers,
      retained,
    });
  }
  // One operation per group/action avoids stale target versions within a bulk request.
  const grouped = new Map<string, DirectoryChangeInput>();
  for (const change of changes) {
    const a = change.action;
    if (a.kind !== "group.members") continue;
    const k = change.connectionId + ":" + a.objectId + ":" + a.action;
    const old = grouped.get(k);
    if (old?.action.kind === "group.members")
      old.action.memberIds = [
        ...new Set([...old.action.memberIds, ...a.memberIds]),
      ];
    else grouped.set(k, change);
  }
  const touched = [...grouped.values()].map(
    (c) =>
      c.connectionId + ":" + ("objectId" in c.action ? c.action.objectId : ""),
  );
  if (new Set(touched).size !== touched.length)
    fail(
      "Split additions and removals to the same group into separate previews.",
      "DIRECTORY_BATCH_DEPENDENCY",
    );
  const directory = [];
  for (const change of grouped.values()) {
    if (
      change.action.kind === "group.members" &&
      change.action.memberIds.length > 100
    )
      fail("Split this request into smaller employee batches.");
    const p = await prepareDirectoryPreview(c, actor, change);
    if (p.evidence.companies.some((id) => id !== actor.companyId))
      fail(
        "Use the Directory console to review changes shared across Companies.",
        "DIRECTORY_APPLY_SCOPE",
        403,
      );
    directory.push({
      change,
      evidence: p.evidence,
      stateVersion: p.state.version,
    });
  }
  if (
    input.revokeOnly &&
    input.revokeApplicationIds.some(
      (id) => !plans.some((p) => p.oldIds.includes(id)),
    )
  )
    fail("Every withdrawn source must belong to the selected employees.");
  const evidence = {
    plans,
    directory,
    inventories: [...inventories.entries()],
    sources,
    beforeApps: beforeApps.map((a) => [a.id, a.ended_at, a.valid_until]),
  };
  const result: ProvisioningPreview = {
    token: digest({ input, evidence }),
    employees: plans.map((p) => ({
      id: p.id,
      name: p.name,
      packageName: p.definition.name,
      revision: p.revision,
      added: [
        ...p.definition.grants.map(
          (g) => "ERPX role (independent review): " + g.roleId,
        ),
        ...p.definition.groupIds.map((id) => "ERPX group: " + id),
        ...p.definition.directoryGroupIds.map((id) => "Directory group: " + id),
      ],
      removed: p.oldIds.flatMap((id) => {
        const old = beforeApps.find((a) => a.id === id);
        return [
          old.definition.name + " · applied revision " + old.package_revision,
        ];
      }),
      retained: p.retained,
      reviews: p.users.length * p.definition.grants.length,
    })),
    warnings: [
      ...(input.restoreAccess
        ? [
            "Restore access after HR placement. All old grants remain revoked; requested ERPX roles need new independent review.",
          ]
        : []),
      "Only the selected employees receive this revision. ERPX roles require an independent reviewer.",
      ...(plans.some((p) => p.definition.windowsPolicies.length)
        ? [
            "GPO targets are shared. Publish or link GPOs separately in Directory & GPO after reviewing all affected people and computers.",
          ]
        : []),
    ],
  };
  return { result, evidence };
}
export async function previewProvisioning(
  raw: PostgresDatabase,
  actor: IamActor,
  input: ProvisioningInput,
) {
  scope(actor);
  const c = await iamDb(raw).pool.connect();
  try {
    await c.query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY");
    return (await buildPreview(c, actor, provisioningInput.parse(input)))
      .result;
  } finally {
    await c.query("ROLLBACK");
    c.release();
  }
}
export async function applyProvisioning(
  raw: PostgresDatabase,
  actor: IamActor,
  input: ProvisioningInput,
  token: string,
  key: string,
) {
  scope(actor);
  await requireFreshMfa(actor);
  input = provisioningInput.parse(input);
  return iamCommand(
    raw,
    actor,
    key,
    "technology.provision.apply",
    input,
    "iam.directory.apply",
    async (c) => {
      const p = await buildPreview(c, actor, input);
      if (p.result.token !== token)
        fail(
          "Review the current employee and resource changes first.",
          "PREVIEW_CHANGED",
        );
      for (const plan of p.evidence.plans)
        if (
          (
            await c.query(
              "SELECT 1 FROM people_provision_operations op, jsonb_array_elements(op.evidence->'plans') p WHERE op.company_id=$1 AND op.status IN ('queued','running','retry','pending_review') AND p->>'personId'=$2",
              [actor.companyId, plan.personId],
            )
          ).rowCount
        )
          fail("This employee already has provisioning in progress.");
      const id = randomUUID(),
        now = Date.now();
      await c.query(
        "INSERT INTO people_provision_operations(id,company_id,definition,evidence,status,created_by,assignment_id,session_id,created_at,updated_at) VALUES($1,$2,$3,$4,'queued',$5,$6,$7,$8,$8)",
        [
          id,
          actor.companyId,
          JSON.stringify(input),
          JSON.stringify(p.evidence),
          actor.userId,
          actor.assignmentId,
          actor.sessionId,
          now,
        ],
      );
      const reviews: string[] = [],
        steps: { id: string; status: string; label: string }[] = [];
      for (const plan of p.evidence.plans) {
        const applicationId = id + ":" + plan.id;
        await c.query(
          "INSERT INTO people_applied_packages(id,company_id,person_id,engagement_id,position_id,package_id,package_revision,definition,source_kind,scope,operation_id,authorizing_assignment_id,valid_from,valid_until,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$13)",
          [
            applicationId,
            actor.companyId,
            plan.personId,
            plan.id,
            plan.positionId,
            plan.packageId,
            plan.revision,
            JSON.stringify(plan.definition),
            input.sourceKind,
            JSON.stringify({
              exclude: input.exclude,
              branchId: plan.branchId,
              departmentId: plan.departmentId,
            }),
            id,
            actor.assignmentId,
            now,
            input.validUntil,
          ],
        );
        for (const userId of plan.users)
          for (const [index, g] of plan.definition.grants.entries()) {
            const grant = grantInput.parse({
                ...g,
                userId,
                validUntil: Math.min(g.validUntil, input.validUntil),
                reason: input.reason,
              }),
              grantId = applicationId + ":" + userId + ":" + index;
            const review = await proposeIamReview(
              c,
              actor,
              "people.provision.grant",
              {
                ...grant,
                operationId: id,
                grantId,
                parentId: actor.assignmentId,
              },
              input.reason,
            );
            await c.query(
              "INSERT INTO people_provision_grants(id,operation_id,application_id,definition,review_id) VALUES($1,$2,$3,$4,$5)",
              [grantId, id, applicationId, JSON.stringify(grant), review.id],
            );
            reviews.push(review.id);
          }
      }
      for (const [index, d] of p.evidence.directory.entries()) {
        const operationId = id + ":directory:" + index;
        await c.query(
          "INSERT INTO people_directory_operations(id,company_id,connection_id,connection_version,state_version,status,definition,evidence,created_by,assignment_id,created_at,updated_at,approval_mode,mfa_session_id,provision_id) VALUES($1,$2,$3,$4,$5,'queued',$6,$7,$8,$9,$10,$10,'direct',$11,$12)",
          [
            operationId,
            actor.companyId,
            d.change.connectionId,
            d.change.expectedVersion,
            d.stateVersion,
            JSON.stringify(d.change),
            JSON.stringify(d.evidence),
            actor.userId,
            actor.assignmentId,
            now,
            actor.sessionId,
            id,
          ],
        );
        steps.push({
          id: operationId,
          status: "queued",
          label:
            d.change.action.kind +
            " · " +
            ("objectId" in d.change.action ? d.change.action.objectId : ""),
        });
      }
      await c.query(
        "UPDATE people_provision_operations SET steps=$2 WHERE id=$1",
        [id, JSON.stringify(steps)],
      );
      return { id, status: "queued", reviews };
    },
  );
}
export async function applyProvisioningReview(
  c: PoolClient,
  actor: IamActor,
  p: Record<string, unknown>,
  decision = "approve",
) {
  const g = (
    await c.query(
      "SELECT g.*,ap.person_id,ap.company_id,ap.ended_at,op.status FROM people_provision_grants g JOIN people_applied_packages ap ON ap.id=g.application_id JOIN people_provision_operations op ON op.id=g.operation_id WHERE g.id=$1 AND ap.company_id=$2 FOR UPDATE OF g",
      [p.grantId, actor.companyId],
    )
  ).rows[0];
  if (
    !g ||
    g.assignment_id ||
    g.ended_at ||
    ["rejected", "blocked"].includes(g.status)
  )
    fail("This applied access request changed.");
  if (decision === "reject") return; // Refusing a role never undoes HR placement or unrelated resources.
  if (
    (
      await c.query("SELECT people_person_user($1,$2,$3) same", [
        actor.companyId,
        g.person_id,
        actor.userId,
      ])
    ).rows[0].same
  )
    fail("Another person must review your access.", "SELF_ACCESS_CHANGE", 403);
  const grant = grantInput.parse(g.definition);
  if (
    !(
      await c.query("SELECT people_person_user($1,$2,$3) ok", [
        actor.companyId,
        g.person_id,
        grant.userId,
      ])
    ).rows[0].ok
  )
    fail("The verified account link changed.");
  await validateIamGrant(c, actor, grant, actor.assignmentId);
  const assignment = await insertIamGrant(
    c,
    actor,
    grant,
    actor.assignmentId,
    "provisioning",
    g.id,
  );
  await c.query(
    "UPDATE people_provision_grants SET assignment_id=$2 WHERE id=$1",
    [g.id, assignment],
  );
}
export async function drainProvisioning(raw: PostgresDatabase) {
  const db = iamDb(raw),
    due = (
      await db.pool.query(
        "SELECT id FROM people_provision_operations WHERE status IN ('queued','running','retry') ORDER BY created_at LIMIT 30",
      )
    ).rows;
  for (const row of due) {
    const c = await db.pool.connect();
    try {
      await c.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
      // Keep package source retirement atomic with the GPO reconciler's final
      // source check. A busy Directory pass defers this tick without failing it.
      const available = (await c.query("SELECT pg_try_advisory_xact_lock(821039320) AS available")).rows[0]?.available;
      if (!available) {
        await c.query("COMMIT");
        break;
      }
      const op = (
        await c.query(
          "SELECT * FROM people_provision_operations WHERE id=$1 AND status IN ('queued','running','retry') FOR UPDATE SKIP LOCKED",
          [row.id],
        )
      ).rows[0];
      if (!op) {
        await c.query("ROLLBACK");
        continue;
      }
      const children = (
        await c.query(
          "SELECT id,status,last_error FROM people_directory_operations WHERE provision_id=$1 ORDER BY id",
          [op.id],
        )
      ).rows;
      const steps = op.steps.map((s: { id: string }) => ({
        ...s,
        status: children.find((j) => j.id === s.id)?.status ?? "blocked",
      }));
      if (children.some((j) => j.status !== "completed")) {
        await c.query(
          "UPDATE people_provision_operations SET status=$2,steps=$3,last_error=$4 WHERE id=$1",
          [
            op.id,
            children.some((j) => j.status === "blocked")
              ? "blocked"
              : "running",
            JSON.stringify(steps),
            children.find((j) => j.last_error)?.last_error ?? null,
          ],
        );
        await c.query("COMMIT");
        continue;
      }
      await requireFreshMfa({
        userId: op.created_by,
        sessionId: op.session_id,
      });
      if (
        !(
          await c.query(
            "SELECT 1 FROM assignments WHERE id=$1 AND user_id=$2 AND company_id=$3 AND scope_kind='holding' AND iam_assignment_active(id,$4) AND iam_role_allows(role,company_id,'iam.directory.apply')",
            [op.assignment_id, op.created_by, op.company_id, Date.now()],
          )
        ).rowCount
      )
        fail(
          "The IT operator's authority changed.",
          "PROVISIONING_AUTHORITY_CHANGED",
        );
      for (const plan of op.evidence.plans as PersonPlan[]) {
        const current = (
          await c.query(
            "SELECT e.version,e.status,p.status person_status FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.id=$1 AND e.company_id=$2 FOR UPDATE OF e",
            [plan.id, op.company_id],
          )
        ).rows[0];
        if (
          !current ||
          current.version !== plan.engagementVersion ||
          (!op.definition.revokeOnly &&
            (current.status !== "active" || current.person_status !== "active"))
        )
          fail("HR placement changed. Preview provisioning again.");
        const cutoff = (
          await c.query(
            "SELECT max(effective_at) at FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3",
            [op.company_id, plan.personId, Date.now()],
          )
        ).rows[0]?.at;
        if (cutoff && !op.definition.revokeOnly) {
          if (
            !op.definition.restoreAccess ||
            !(
              await c.query(
                "SELECT 1 FROM people_lifecycle_cases WHERE company_id=$1 AND engagement_id=$2 AND status='completed' AND workflow='placement' AND kind IN ('onboarding','transfer') AND effective_at>$3",
                [op.company_id, plan.id, cutoff],
              )
            ).rowCount
          )
            fail("This employee's access was suspended.");
          await c.query(
            "UPDATE assignments SET revoked_at=$3,version=version+1 WHERE company_id=$1 AND people_person_user($1,$2,user_id) AND revoked_at IS NULL AND NOT EXISTS(SELECT 1 FROM people_provision_grants g WHERE g.operation_id=$4 AND g.assignment_id=assignments.id)",
            [op.company_id, plan.personId, Date.now(), op.id],
          );
          await c.query(
            "UPDATE people_applied_packages SET ended_at=$3 WHERE company_id=$1 AND person_id=$2 AND operation_id IS DISTINCT FROM $4 AND ended_at IS NULL",
            [op.company_id, plan.personId, Date.now(), op.id],
          );
          await c.query(
            "UPDATE people_access_cutoffs SET restored_at=$3 WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3",
            [op.company_id, plan.personId, Date.now()],
          );
        }
        const now = Date.now(),
          applicationId = op.id + ":" + plan.id;
        if (!op.definition.revokeOnly)
          await c.query(
            "UPDATE people_preparation_tasks t SET status='completed',version=t.version+1,updated_at=$3 FROM people_lifecycle_cases lc WHERE t.case_id=lc.id AND lc.company_id=$1 AND lc.engagement_id=$2 AND lc.workflow='placement' AND lc.status='completed' AND lc.kind IN ('onboarding','transfer') AND t.title='Prepare access for employee placement' AND t.status='pending'",
            [op.company_id, plan.id, now],
          );
        if (op.definition.revokeOnly)
          await c.query(
            "UPDATE people_applied_packages SET ended_at=$2 WHERE id=$1",
            [applicationId, now],
          );
        // Retire only the replaced source; manual and supplemental assignments keep their own expiry.
        await c.query(
          "UPDATE people_applied_packages SET ended_at=$2 WHERE id=ANY($1::text[]) AND ended_at IS NULL",
          [plan.oldIds, now],
        );
        await c.query(
          "DELETE FROM people_directory_membership_sources WHERE application_id=ANY($1::text[])",
          [plan.oldIds],
        );
        for (const groupId of plan.definition.directoryGroupIds)
          for (const memberId of plan.directoryMembers) {
            const wasMember = op.evidence.inventories.some(
              ([id, state]: [string, { inventory: DirectoryInventory }]) =>
                id === plan.definition.connectionId &&
                state.inventory.objects.some(
                  (o) => o.id === groupId && o.memberIds.includes(memberId),
                ),
            );
            const tracked = op.evidence.sources.some(
              (s: {
                connection_id: string;
                group_id: string;
                member_id: string;
              }) =>
                s.connection_id === plan.definition.connectionId &&
                s.group_id === groupId &&
                s.member_id === memberId,
            );
            if (wasMember && !tracked)
              await c.query(
                "INSERT INTO people_directory_membership_sources VALUES($1,$2,$3,'external',NULL,$4) ON CONFLICT DO NOTHING",
                [plan.definition.connectionId, groupId, memberId, now],
              );
            await c.query(
              "INSERT INTO people_directory_membership_sources VALUES($1,$2,$3,$4,$4,$5) ON CONFLICT DO NOTHING",
              [
                plan.definition.connectionId,
                groupId,
                memberId,
                applicationId,
                now,
              ],
            );
          }
        for (const [index, task] of [
          ...plan.definition.tasks,
          ...plan.definition.windowsPolicies.map((p) => ({
            kind: "directory",
            label: "Verify shared GPO · " + p.name,
          })),
        ].entries())
          await c.query(
            "INSERT INTO people_preparation_tasks(id,company_id,kind,title,updated_at,privileged,application_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
            [
              applicationId + ":task:" + index,
              op.company_id,
              task.kind,
              task.label,
              now,
              !["equipment", "handover"].includes(task.kind),
              applicationId,
            ],
          );
      }
      await c.query(
        "UPDATE people_provision_operations SET status='completed',steps=$2,version=version+1,updated_at=$3,last_error=NULL WHERE id=$1",
        [op.id, JSON.stringify(steps), Date.now()],
      );
      await c.query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$1,'people.provision.applied','people_provision',$2,1,$3,$4) ON CONFLICT DO NOTHING",
        [
          "provision:" + op.id,
          op.id,
          JSON.stringify({ id: op.id, companyId: op.company_id }),
          Date.now(),
        ],
      );
      await c.query(
        "INSERT INTO iam_audit VALUES($1,$2,$3,$4,'people.provision.applied',$5,$6,$7)",
        [
          randomUUID(),
          op.company_id,
          op.created_by,
          op.assignment_id,
          op.id,
          JSON.stringify({ steps }),
          Date.now(),
        ],
      );
      await c.query("COMMIT");
    } catch (error) {
      await c.query("ROLLBACK");
      await c.query(
        "UPDATE people_provision_operations SET status='blocked',last_error=$2,version=version+1,updated_at=$3 WHERE id=$1",
        [
          row.id,
          error instanceof ApiError
            ? error.code
            : "PROVISIONING_RETRY_REQUIRED",
          Date.now(),
        ],
      );
    } finally {
      c.release();
    }
  }
}

export async function retryProvisioning(
  raw: PostgresDatabase,
  actor: IamActor,
  id: string,
  version: number,
  key: string,
) {
  scope(actor);
  await requireFreshMfa(actor);
  return iamCommand(
    raw,
    actor,
    key,
    "technology.provision.retry",
    { id, version },
    "iam.directory.apply",
    async (c) => {
      const op = (
        await c.query(
          "SELECT * FROM people_provision_operations WHERE id=$1 AND company_id=$2 FOR UPDATE",
          [id, actor.companyId],
        )
      ).rows[0];
      if (
        !op ||
        op.created_by !== actor.userId ||
        op.version !== version ||
        !["retry", "blocked"].includes(op.status) ||
        !["FRESH_MFA_REQUIRED", "PROVISIONING_RETRY_REQUIRED"].includes(
          op.last_error,
        )
      )
        fail(
          "Read the current state and preview a new change. Only an interrupted operation can resume.",
        );
      await c.query(
        "UPDATE people_provision_operations SET status='queued',session_id=$2,version=version+1,last_error=NULL,updated_at=$3 WHERE id=$1",
        [id, actor.sessionId, Date.now()],
      );
      return { id, status: "queued" };
    },
  );
}
