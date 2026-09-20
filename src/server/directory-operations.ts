import "server-only";
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import type { PoolClient } from "pg";
import type { PostgresDatabase } from "./runtime/postgres";
import { iamDb } from "./iam-db";
import { assertCompanyScope, type IamActor } from "./iam-context";
import { requirePermission } from "./iam-permissions";
import { hasPermission } from "@/lib/permissions";
import { iamCommand } from "./iam-transactions";
import { requireFreshMfa } from "./iam-mfa";
import { proposeIamReview } from "./iam-service";
import { type ConnectionRow } from "./iam-directories";
import {
  readAutomationInventory,
  validateObjectTarget,
  applyDirectoryAction,
  providerFailure as fail,
} from "./directory-provider";
import {
  directoryChangeInput,
  type DirectoryAction,
  type DirectoryInventory,
  type DirectoryChangeInput,
  type DirectoryAutomationData,
  type DirectoryPreview,
} from "@/lib/directory-operations";
import { ApiError } from "./http";
import { attributeAccess } from "@/lib/user-management";

type State = {
  connection_id: string;
  company_id: string;
  version: number;
  tested_version: number | null;
  write_enabled: boolean;
  inventory: DirectoryInventory | null;
  definition: Record<string, unknown>;
};
function scope(actor: IamActor, permission = "iam.directory.manage") {
  if (actor.platform)
    fail("COMPANY_SCOPE_REQUIRED", "Use People in a Company workspace.", 403);
  assertCompanyScope(actor);
  requirePermission(actor, permission);
}
function canonical(v: unknown): unknown {
  return Array.isArray(v)
    ? v.map(canonical)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.entries(v)
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([k, x]) => [k, canonical(x)]),
        )
      : v;
}
const hash = (v: unknown) =>
  createHash("sha256")
    .update(JSON.stringify(canonical(v)))
    .digest("hex");
async function connection(
  c: PoolClient,
  actor: Pick<IamActor, "companyId">,
  id: string,
  readOnly = false,
) {
  const row = (
    await c.query(
      "SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2",
      [id, actor.companyId],
    )
  ).rows[0] as ConnectionRow | undefined;
  if (
    !row ||
    !(readOnly ? ["draft", "tested", "active"] : ["active"]).includes(
      row.status,
    )
  )
    fail("CONNECTION_UNAVAILABLE", "Choose an available directory connection.");
  return row;
}
export async function directoryAutomationData(
  db: PostgresDatabase,
  actor: IamActor,
): Promise<DirectoryAutomationData> {
  scope(actor, "iam.read");
  db = iamDb(db);
  const rows = (
    await db.pool.query(
      "SELECT c.*,s.version state_version,s.tested_version automation_tested,s.write_enabled,s.inventory,s.definition FROM iam_connections c LEFT JOIN people_directory_state s ON s.connection_id=c.id WHERE c.company_id=$1 AND c.status IN ('draft','tested','active') AND c.provider IN ('univention','active_directory') ORDER BY c.name",
      [actor.companyId],
    )
  ).rows;
  const jobs = (
    await db.pool.query(
      "SELECT id,connection_id,status,version,definition,last_error,result,review_id,created_at,approval_mode,steps FROM people_directory_operations WHERE company_id=$1 ORDER BY created_at DESC LIMIT 200",
      [actor.companyId],
    )
  ).rows;
  const cases = (
    await db.pool.query(
      "SELECT c.id,p.legal_name name,c.kind,c.effective_at,ARRAY(SELECT i.external_id FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE m.person_id=c.person_id AND m.company_id=c.company_id AND m.status='active') account_ids FROM people_lifecycle_cases c JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id WHERE c.company_id=$1 AND c.status IN ('scheduled','completed') ORDER BY c.updated_at DESC LIMIT 200",
      [actor.companyId],
    )
  ).rows;
  return {
    cases: cases.map((c) => ({
      id: c.id,
      name: c.name,
      kind: c.kind,
      effectiveAt: c.effective_at,
      accountIds: c.account_ids,
    })),
    connections: rows.map((r) => ({
      id: r.id,
      name: r.name,
      provider: r.provider,
      version: r.version,
      writeEnabled: r.write_enabled === true,
      testedVersion: r.automation_tested ?? null,
      inventory: r.inventory ?? null,
      mappings: r.definition?.mappings ?? [],
    })),
    jobs,
    canApply: hasPermission(actor, "iam.directory.apply"),
    canManage: hasPermission(actor, "iam.directory.manage"),
    canReview: hasPermission(actor, "iam.review"),
  };
}
export async function refreshDirectoryInventory(
  db: PostgresDatabase,
  actor: IamActor,
  id: string,
  version: number,
  key: string,
) {
  scope(actor);
  db = iamDb(db);
  const row = await iamCommand(
    db,
    actor,
    key + ":authorize",
    "directory.inventory.authorize",
    { id, version },
    "iam.directory.manage",
    async (c) => {
      const row = await connection(c, actor, id, true);
      if (row.version !== version)
        fail("CONNECTION_CHANGED", "Refresh the connection first.");
      return row;
    },
  );
  const inventory = await readAutomationInventory(row);
  return iamCommand(
    db,
    actor,
    key,
    "directory.inventory.read",
    { id, version },
    "iam.directory.manage",
    async (c) => {
      const current = await connection(c, actor, id, true);
      if (current.version !== version)
        fail(
          "CONNECTION_CHANGED",
          "The connection changed while reading the inventory.",
        );
      await c.query(
        "INSERT INTO people_directory_state(connection_id,company_id,tested_version,inventory,observed_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(connection_id) DO UPDATE SET tested_version=excluded.tested_version,inventory=excluded.inventory,observed_at=excluded.observed_at,write_enabled=CASE WHEN people_directory_state.tested_version=excluded.tested_version AND people_directory_state.inventory->>'providerVersion'=excluded.inventory->>'providerVersion' THEN people_directory_state.write_enabled ELSE false END",
        [id, actor.companyId, version, JSON.stringify(inventory), Date.now()],
      );
      return {
        count: inventory.objects.length,
        providerVersion: inventory.providerVersion,
      };
    },
  );
}
export const connectorMappingInput = z.object({
  attributeId: z.string().min(1),
  field: z.string().min(1),
  direction: z.enum(["read", "write"]),
});
export async function configureDirectoryAutomation(
  db: PostgresDatabase,
  actor: IamActor,
  input: {
    connectionId: string;
    expectedVersion: number;
    writeEnabled: boolean;
    mappings?: z.infer<typeof connectorMappingInput>[];
  },
  key: string,
) {
  scope(actor);
  await requireFreshMfa(actor);
  return iamCommand(
    db,
    actor,
    key,
    "directory.automation.configure",
    input,
    "iam.directory.manage",
    async (c) => {
      const row = await connection(c, actor, input.connectionId),
        state = (
          await c.query(
            "SELECT * FROM people_directory_state WHERE connection_id=$1 FOR UPDATE",
            [row.id],
          )
        ).rows[0] as State | undefined;
      if (
        row.version !== input.expectedVersion ||
        !state?.inventory ||
        state.tested_version !== row.version
      )
        fail(
          "DIRECTORY_TEST_REQUIRED",
          "Test this connection version and its schema before configuring write-back.",
        );
      const mappings = (
        input.mappings ??
        (state.definition.mappings as z.infer<
          typeof connectorMappingInput
        >[]) ??
        []
      ).map((m) => connectorMappingInput.parse(m));
      if (
        new Set(mappings.map((m) => m.attributeId)).size !== mappings.length ||
        new Set(mappings.map((m) => m.field)).size !== mappings.length
      )
        fail(
          "MAPPING_DUPLICATE",
          "Map each local and external field once.",
          422,
        );
      for (const m of mappings) {
        const a = (
          await c.query(
            "SELECT definition FROM um_attributes WHERE company_id=$1 AND id=$2",
            [actor.companyId, m.attributeId],
          )
        ).rows[0];
        if (
          !a ||
          a.definition.target !== "person" ||
          a.definition.archived ||
          !["text", "checkbox"].includes(a.definition.type)
        )
          fail(
            "MAPPING_ATTRIBUTE_INVALID",
            "Choose an active compatible person field.",
            422,
          );
        const access = attributeAccess(
          a.definition,
          actor.role,
          actor.scopeKind,
        );
        if (
          !access.canView ||
          (m.direction === "read" &&
            (!access.canEdit || !hasPermission(actor, "people.manage")))
        )
          fail(
            "MAPPING_FIELD_FORBIDDEN",
            "The mapping requires permission to view or update the owned field.",
            403,
          );
        const f = state.inventory.fields.find(
          (f) =>
            f.name === m.field &&
            f[m.direction] &&
            f.type === a.definition.type,
        );
        if (!f)
          fail(
            "MAPPING_SCHEMA_UNSUPPORTED",
            "This connector schema does not support the field and direction.",
            422,
          );
      }
      await c.query(
        "UPDATE people_directory_state SET write_enabled=$1,definition=$2,version=version+1 WHERE connection_id=$3",
        [input.writeEnabled, JSON.stringify({ mappings }), row.id],
      );
      return { writeEnabled: input.writeEnabled };
    },
  );
}
export function affectedObjects(
  action: DirectoryAction,
  inventory: DirectoryInventory,
) {
  const ids = new Set<string>();
  if ("objectId" in action && action.objectId) ids.add(action.objectId);
  if ("parentId" in action) ids.add(action.parentId);
  if (action.kind === "group.members")
    action.memberIds.forEach((id) => ids.add(id));
  if (action.kind === "gpo.publish") {
    if (!action.objectId) {
      ids.add(action.definition.targetOuId);
      ids.add(action.definition.filterGroupId);
    }
    if (action.definition.computerTargetId)
      ids.add(action.definition.computerTargetId);
  }
  if (action.kind === "gpo.link" && action.filterGroupId)
    ids.add(action.filterGroupId);
  if (action.kind === "gpo.filter")
    action.groupIds.forEach((id) => ids.add(id));
  if (action.kind === "gpo.order") action.gpoIds.forEach((id) => ids.add(id));
  for (let changed = true; changed;) {
    changed = false;
    for (const o of inventory.objects)
      if (ids.has(o.id)) {
        for (const id of [
          ...o.memberIds,
          ...(o.kind === "gpo" &&
          Array.isArray(o.properties.securityFilterGroupIds)
            ? (o.properties.securityFilterGroupIds as string[])
            : []),
        ])
          if (!ids.has(id)) {
            ids.add(id);
            changed = true;
          }
      } else if (
        (o.parentId && ids.has(o.parentId)) ||
        (o.kind === "group" && o.memberIds.some((id) => ids.has(id)))
      ) {
        ids.add(o.id);
        changed = true;
      }
  }
  return inventory.objects.filter((o) => ids.has(o.id));
}
export async function prepareDirectoryPreview(
  c: PoolClient,
  actor: IamActor,
  input: DirectoryChangeInput,
) {
  const row = await connection(c, actor, input.connectionId, true),
    state = (
      await c.query(
        "SELECT * FROM people_directory_state WHERE connection_id=$1",
        [row.id],
      )
    ).rows[0] as State | undefined;
  if (row.version !== input.expectedVersion)
    fail("CONNECTION_CHANGED", "The connection changed.");
  if (!state?.inventory || state.tested_version !== row.version)
    fail(
      "DIRECTORY_TEST_REQUIRED",
      "Read and test the directory inventory first.",
    );
  const actions = [input.action, ...(input.additionalActions ?? [])];
  const simulated = structuredClone(state.inventory);
  for (const action of actions) {
    validateObjectTarget(action, simulated);
    if (
      row.provider === "univention" &&
      action.kind === "object.move" &&
      simulated.objects.find((o) => o.id === action.objectId)?.kind === "ou" &&
      simulated.objects.some((o) => o.parentId === action.objectId)
    )
      fail(
        "DIRECTORY_CONTAINER_NOT_EMPTY",
        "UCS recreates populated containers when moving them. Move the children first, then move the empty container and return the children to preserve permanent identities.",
      );
    if (action.kind === "object.move")
      simulated.objects.find((o) => o.id === action.objectId)!.parentId =
        action.parentId;
    if (action.kind === "group.members") {
      const group = simulated.objects.find((o) => o.id === action.objectId)!;
      group.memberIds =
        action.action === "add"
          ? [...new Set([...group.memberIds, ...action.memberIds])]
          : group.memberIds.filter((id) => !action.memberIds.includes(id));
    }
  }
  // Each target may be mutated once per batch. Dependent edits require a new preview.
  if (
    actions.length > 1 &&
    actions.some(
      (a) => a.kind === "account.enable" || a.kind === "account.disable",
    )
  )
    fail(
      "DIRECTORY_BATCH_DEPENDENCY",
      "Apply account activation or suspension separately to review its full impact.",
    );
  const targets = actions.flatMap((a) =>
    "objectId" in a && a.objectId ? [a.objectId] : [],
  );
  if (new Set(targets).size !== targets.length)
    fail(
      "DIRECTORY_BATCH_DEPENDENCY",
      "Apply one change per object, then refresh before dependent changes.",
    );
  const objects = [
      ...new Map(
        actions
          .flatMap((a) => affectedObjects(a, state.inventory!))
          .map((o) => [o.id, o]),
      ).values(),
    ],
    ids = objects.flatMap((o) => [
      o.id,
      ...(typeof (o.properties.windowsIdentity as { id?: unknown } | undefined)
        ?.id === "string"
        ? [(o.properties.windowsIdentity as { id: string }).id]
        : []),
    ]);
  const identities = (
    await c.query(
      "SELECT i.*,c.company_id,c.config FROM iam_identities i JOIN iam_connections c ON c.id=i.connection_id WHERE c.config->>'authority'=$1 AND i.external_id=ANY($2::text[]) ORDER BY c.company_id,i.id",
      [row.config.authority, ids],
    )
  ).rows;
  if (
    input.action.kind === "account.enable" ||
    (input.action.kind === "account.disable" && input.action.caseId)
  ) {
    const action = input.action;
    const lifecycle = (
      await c.query(
        "SELECT c.*,(people_case_active(c.id,$3) OR c.workflow='placement' AND c.status='completed' AND c.effective_at<=$3 AND EXISTS(SELECT 1 FROM people_applied_packages ap WHERE ap.company_id=c.company_id AND ap.person_id=c.person_id AND people_application_active(ap.id,$3))) active FROM people_lifecycle_cases c WHERE c.id=$1 AND c.company_id=$2",
        [action.caseId, actor.companyId, Date.now()],
      )
    ).rows[0];
    const linked =
      lifecycle &&
      identities.some(
        (i) =>
          i.external_id === action.objectId &&
          i.user_id &&
          i.company_id === actor.companyId &&
          i.person_id === lifecycle.person_id,
      );
    const verified =
      lifecycle &&
      (
        await c.query(
          "SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE i.connection_id=$1 AND i.external_id=$2 AND m.company_id=$3 AND m.person_id=$4 AND m.status='active'",
          [row.id, action.objectId, actor.companyId, lifecycle.person_id],
        )
      ).rowCount;
    if (!lifecycle || (!linked && !verified) || !verified)
      fail(
        "VERIFIED_LIFECYCLE_REQUIRED",
        "Choose the lifecycle case for this explicitly verified account.",
      );
    if (
      action.kind === "account.enable" &&
      (!lifecycle.active || lifecycle.status !== "completed")
    )
      fail(
        "LIFECYCLE_NOT_EFFECTIVE",
        "Activation requires completed personal access review and the effective start time.",
      );
    if (
      action.kind === "account.disable" &&
      (!["offboarding", "suspend"].includes(lifecycle.kind) ||
        !["scheduled", "completed"].includes(lifecycle.status) ||
        lifecycle.effective_at > Date.now())
    )
      fail(
        "LIFECYCLE_NOT_EFFECTIVE",
        "The reviewed departure cutoff has not taken effect.",
      );
  }
  const relatedConnections = (
    await c.query(
      "SELECT id,company_id FROM iam_connections WHERE config->>'authority'=$1",
      [row.config.authority],
    )
  ).rows;
  const positionRows = (
    await c.query(
      "SELECT DISTINCT p.id,p.company_id,p.title FROM organization_positions p JOIN people_position_packages b ON b.position_id=p.id AND b.company_id=p.company_id JOIN people_package_revisions r ON r.package_id=b.package_id AND r.revision=b.revision AND r.company_id=b.company_id WHERE r.definition->>'connectionId'=ANY($1::text[]) AND (EXISTS(SELECT 1 FROM jsonb_array_elements_text(r.definition->'directoryGroupIds') g WHERE g=ANY($2::text[])) OR EXISTS(SELECT 1 FROM jsonb_array_elements(r.definition->'windowsPolicies') w WHERE w->>'id'=ANY($2::text[]))) ORDER BY p.title",
      [relatedConnections.map((r) => r.id), ids],
    )
  ).rows;
  const positions = positionRows.map((r) => String(r.title));
  const companyIds = [
    ...new Set([
      actor.companyId,
      ...identities.map((i) => String(i.company_id)),
      ...positionRows.map((r) => String(r.company_id)),
    ]),
  ].sort();
  const companyNames = (
    await c.query(
      "SELECT company_id id,display_name name FROM organization_nodes WHERE company_id=ANY($1::text[]) AND kind='company' ORDER BY company_id",
      [companyIds],
    )
  ).rows;
  const companies = companyIds.map((id) => ({
    id,
    name: String(companyNames.find((r) => r.id === id)?.name ?? id),
  }));
  if (input.action.kind === "account.disable") {
    const targetId = input.action.objectId,
      target = state.inventory.objects.find((o) => o.id === targetId),
      aliases = [
        targetId,
        (target?.properties.windowsIdentity as { id?: string } | undefined)?.id,
      ],
      accounts = identities.filter(
        (i) => aliases.includes(i.external_id) && i.user_id,
      );
    for (const i of accounts) {
      const protectedAccount = (
        await c.query(
          "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$1 AND a.company_id=$2 AND a.role='holding_owner' AND iam_assignment_active(a.id,$3)) OR EXISTS(SELECT 1 FROM iam_members m WHERE m.user_id=$1 AND m.company_id=$2 AND m.login_policy='emergency' AND m.status='active') protected,iam_same_person($1,$4,$2) self",
          [i.user_id, i.company_id, Date.now(), actor.userId],
        )
      ).rows[0];
      if (protectedAccount.protected || protectedAccount.self)
        fail(
          "PROTECTED_DIRECTORY_ACCOUNT",
          "This operation would disable your own account or a recovery administrator.",
          403,
        );
    }
  }
  const people = (
    await c.query(
      "SELECT DISTINCT p.legal_name FROM hr_people p LEFT JOIN iam_members m ON m.person_id=p.id AND m.company_id=p.company_id WHERE p.user_id=ANY($1::text[]) OR m.user_id=ANY($1::text[]) OR EXISTS(SELECT 1 FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.position_id=ANY($2::text[]) AND e.status='active') ORDER BY p.legal_name",
      [
        identities.map((i) => i.user_id).filter(Boolean),
        positionRows.map((p) => p.id),
      ],
    )
  ).rows.map((r) => String(r.legal_name));

  const resultSources: string[] = [];
  for (const action of actions)
    if (action.kind === "group.members" && action.action === "remove") {
      const claims = (
        await c.query(
          "SELECT s.member_id,s.source_id FROM people_directory_membership_sources s LEFT JOIN people_applied_packages ap ON ap.id=s.application_id WHERE s.connection_id=$1 AND s.group_id=$2 AND s.member_id=ANY($3::text[]) AND s.application_id IS NOT NULL AND ap.ended_at IS NULL",
          [row.id, action.objectId, action.memberIds],
        )
      ).rows;
      // Preview exposes sources. The provisioning service removes only its replaced claims.
      if (claims.length)
        resultSources.push(
          ...claims.map(
            (v) =>
              `Membership ${v.member_id} is supplied by applied package ${v.source_id}.`,
          ),
        );
    }
  const evidence = {
    connectionVersion: row.version,
    stateVersion: state.version,
    inventory: state.inventory,
    objects: ids,
    companies: companyIds,
    identityLinks: identities.map((i) => [i.id, i.user_id, i.company_id]),
  };
  const result: DirectoryPreview = {
    token: hash({ input, evidence }),
    objects: objects.map((o) => o.name + " · " + o.kind),
    companies,
    people,
    computers: objects.filter((o) => o.kind === "computer").map((o) => o.name),
    positions,
    warnings: resultSources,
  };
  if (!state.write_enabled)
    result.warnings.push(
      "Read-only connection: IT can preview work; enable tested write-back before submission.",
    );
  if (input.action.kind === "object.move")
    result.warnings.push(
      "Moving an OU placement changes inherited Windows policies.",
    );
  if (input.action.kind === "account.disable" && companyIds.length > 1)
    result.warnings.push(
      "Disabling this directory account affects every listed Company.",
    );
  if (input.action.kind === "gpo.publish")
    result.warnings.push(
      "Published GPO configuration is separate from confirmed results on endpoints.",
    );
  return { result, evidence, state, row };
}
export async function previewDirectoryChange(
  db: PostgresDatabase,
  actor: IamActor,
  input: DirectoryChangeInput,
) {
  scope(actor);
  const c = await iamDb(db).pool.connect();
  try {
    await c.query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY");
    return (await prepareDirectoryPreview(c, actor, input)).result;
  } finally {
    await c.query("ROLLBACK");
    c.release();
  }
}
export async function submitDirectoryChange(
  db: PostgresDatabase,
  actor: IamActor,
  raw: DirectoryChangeInput,
  token: string,
  key: string,
  direct = false,
) {
  scope(actor, direct ? "iam.directory.apply" : "iam.directory.manage");
  await requireFreshMfa(actor);
  const input = directoryChangeInput.parse(raw);
  return iamCommand(
    db,
    actor,
    key,
    direct ? "directory.operation.apply" : "directory.operation.propose",
    input,
    direct ? "iam.directory.apply" : "iam.directory.manage",
    async (c) => {
      const p = await prepareDirectoryPreview(c, actor, input);
      if (p.result.token !== token)
        fail(
          "DIRECTORY_PREVIEW_CHANGED",
          "Review the latest directory impact before submitting.",
        );
      if (!p.state.write_enabled)
        fail(
          "DIRECTORY_READ_ONLY",
          "Enable tested write-back or keep this as a manual IT task.",
        );
      if (p.row.status !== "active")
        fail(
          "DIRECTORY_READ_ONLY",
          "Activate the tested connection through independent review first.",
        );
      if (
        input.action.kind === "account.disable" &&
        p.result.companies.some((c) => !input.affectedCompanyIds.includes(c.id))
      )
        fail(
          "DIRECTORY_IMPACT_ACK_REQUIRED",
          "Acknowledge every affected Company before disabling the whole account.",
        );
      if (direct)
        for (const companyId of p.evidence.companies) {
          if (
            !(
              await c.query(
                "SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) AND iam_role_allows(a.role,a.company_id,'iam.directory.apply')",
                [companyId, actor.userId, Date.now()],
              )
            ).rowCount
          )
            fail(
              "DIRECTORY_APPLY_SCOPE",
              "You must administer every affected Company.",
              403,
            );
        }
      const id = randomUUID(),
        review = direct
          ? null
          : await proposeIamReview(
              c,
              actor,
              "directory.operation",
              { operationId: id },
              input.reason,
            );
      await c.query(
        "INSERT INTO people_directory_operations(id,company_id,connection_id,connection_version,state_version,status,definition,evidence,review_id,created_by,assignment_id,created_at,updated_at,approval_mode,mfa_session_id) VALUES($1,$2,$3,$4,$5,$12,$6,$7,$8,$9,$10,$11,$11,$13,$14)",
        [
          id,
          actor.companyId,
          p.row.id,
          p.row.version,
          p.state.version,
          JSON.stringify(input),
          JSON.stringify(p.evidence),
          review?.id ?? null,
          actor.userId,
          actor.assignmentId,
          Date.now(),
          direct ? "queued" : "pending_review",
          direct ? "direct" : "reviewed",
          direct ? actor.sessionId : null,
        ],
      );
      return {
        id,
        reviewId: review?.id ?? null,
        status: direct ? "queued" : "pending_review",
      };
    },
  );
}
export async function applyDirectoryOperationReview(
  c: PoolClient,
  actor: IamActor,
  p: Record<string, unknown>,
  decision: "approve" | "reject" = "approve",
) {
  const job = (
    await c.query(
      "SELECT * FROM people_directory_operations WHERE id=$1 AND company_id=$2 AND status='pending_review' FOR UPDATE",
      [p.operationId, actor.companyId],
    )
  ).rows[0];
  if (!job)
    fail("DIRECTORY_OPERATION_CHANGED", "This directory operation changed.");
  if (decision === "approve") {
    requirePermission(actor, "iam.directory.manage");
    const current = await prepareDirectoryPreview(c, actor, job.definition);
    if (
      hash(current.evidence) !== hash(job.evidence) ||
      !current.state.write_enabled
    )
      fail(
        "DIRECTORY_PREVIEW_CHANGED",
        "The directory preview changed. Cancel and submit a fresh proposal.",
      );
    if (
      ["account.enable", "group.members"].includes(job.definition.action.kind)
    )
      for (const [, userId, companyId] of current.evidence.identityLinks)
        if (
          userId &&
          (
            await c.query("SELECT iam_same_person($1,$2,$3) same", [
              userId,
              actor.userId,
              companyId,
            ])
          ).rows[0].same
        )
          fail(
            "DIRECTORY_BENEFICIARY_REVIEW",
            "A person affected by this access change cannot approve it.",
            403,
          );
    for (const companyId of current.evidence.companies)
      if (
        companyId !== actor.companyId &&
        !(
          await c.query(
            "SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) AND iam_role_allows(a.role,a.company_id,'iam.directory.manage')",
            [companyId, actor.userId, Date.now()],
          )
        ).rowCount
      )
        fail(
          "DIRECTORY_REVIEW_SCOPE",
          "The reviewer must cover every affected Company.",
          403,
        );
  }
  await c.query(
    "UPDATE people_directory_operations SET status=$1,version=version+1,updated_at=$2 WHERE id=$3",
    [decision === "approve" ? "queued" : "rejected", Date.now(), job.id],
  );
}
/** Transport/MFA retry keeps its journal and completed steps. Stale previews cannot be retried. */
export async function retryDirectoryOperation(
  db: PostgresDatabase,
  actor: IamActor,
  id: string,
  version: number,
  key: string,
) {
  scope(actor, "iam.directory.apply");
  await requireFreshMfa(actor);
  return iamCommand(
    db,
    actor,
    key,
    "directory.operation.retry",
    { id, version },
    "iam.directory.apply",
    async (c) => {
      const row = (
        await c.query(
          "UPDATE people_directory_operations SET status='queued',mfa_session_id=$5,next_attempt_at=0,version=version+1 WHERE id=$1 AND company_id=$2 AND version=$3 AND created_by=$4 AND approval_mode='direct' AND (status='retry' OR status='blocked' AND last_error='FRESH_MFA_REQUIRED') RETURNING provision_id",
          [id, actor.companyId, version, actor.userId, actor.sessionId],
        )
      ).rows[0];
      if (!row)
        fail(
          "DIRECTORY_RETRY_UNAVAILABLE",
          "A stale or uncertain change needs a new preview. Only the original operator can resume a transport or MFA retry.",
        );
      if (row.provision_id)
        await c.query(
          "UPDATE people_provision_operations SET status='queued',session_id=$2,last_error=NULL WHERE id=$1 AND status IN ('blocked','running')",
          [row.provision_id, actor.sessionId],
        );
      return { id, status: "queued" };
    },
  );
}
export async function cancelDirectoryOperation(
  db: PostgresDatabase,
  actor: IamActor,
  id: string,
  version: number,
  key: string,
) {
  scope(actor);
  return iamCommand(
    db,
    actor,
    key,
    "directory.operation.cancel",
    { id, version },
    "iam.directory.manage",
    async (c) => {
      const job = (
        await c.query(
          "UPDATE people_directory_operations SET status='cancelled',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4 AND attempts=0 AND status IN ('pending_review','queued','blocked') RETURNING review_id",
          [Date.now(), id, actor.companyId, version],
        )
      ).rows[0];
      if (!job)
        fail(
          "DIRECTORY_CANCEL_UNAVAILABLE",
          "Only work that has not started can be cancelled.",
        );
      await c.query(
        "UPDATE iam_reviews SET status='failed',version=version+1 WHERE id=$1 AND status='pending'",
        [job.review_id],
      );
      return { id, status: "cancelled" };
    },
  );
}
/** A session advisory lock serializes external operations per connection across workers. */
export async function drainDirectoryOperations(
  db: PostgresDatabase,
  limit = 10,
) {
  db = iamDb(db);
  const due = (
    await db.pool.query(
      "SELECT id,connection_id FROM people_directory_operations WHERE status IN ('queued','retry','running') AND next_attempt_at<=$1 ORDER BY created_at LIMIT $2",
      [Date.now(), limit],
    )
  ).rows;
  for (const row of due) {
    const c = await db.pool.connect();
    let locked = false;
    try {
      locked = (
        await c.query("SELECT pg_try_advisory_lock(hashtext($1)) locked", [
          "people-directory:" + row.connection_id,
        ])
      ).rows[0].locked;
      if (!locked) continue;
      const job = (
        await c.query(
          "SELECT * FROM people_directory_operations WHERE id=$1 AND status IN ('queued','retry','running')",
          [row.id],
        )
      ).rows[0];
      if (!job) continue;
      if (job.provision_id) {
        const op = (
          await c.query(
            "SELECT * FROM people_provision_operations WHERE id=$1",
            [job.provision_id],
          )
        ).rows[0];
        if (!op || !["queued", "running", "retry"].includes(op.status))
          fail(
            "PROVISIONING_CHANGED",
            "This provisioning operation is no longer executable.",
          );
        for (const plan of op.evidence.plans) {
          if (
            !(
              await c.query(
                "SELECT 1 FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.id=$1 AND e.company_id=$2 AND e.version=$3 AND ($6::boolean OR e.status='active' AND p.status='active') AND p.merged_into_id IS NULL AND ($6::boolean OR NOT EXISTS(SELECT 1 FROM people_access_cutoffs x WHERE x.company_id=e.company_id AND x.person_id=e.person_id AND x.cancelled_at IS NULL AND x.restored_at IS NULL AND x.effective_at<=$4 AND NOT ($5::boolean AND EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.workflow='placement' AND lc.status='completed' AND lc.kind IN ('onboarding','transfer') AND lc.effective_at>x.effective_at))))",
                [
                  plan.id,
                  job.company_id,
                  plan.engagementVersion,
                  Date.now(),
                  op.definition.restoreAccess === true,
                  op.definition.revokeOnly === true,
                ],
              )
            ).rowCount
          )
            fail(
              "PROVISIONING_CHANGED",
              "HR placement or employee access changed. Preview again.",
            );
        }
      }
      const conn = await connection(
        c,
        { companyId: job.company_id },
        job.connection_id,
      );
      const state = (
        await c.query(
          "SELECT * FROM people_directory_state WHERE connection_id=$1",
          [conn.id],
        )
      ).rows[0] as State;
      const allowed = (
        await c.query(
          "SELECT 1 FROM assignments a WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND iam_assignment_active(a.id,$4) AND iam_role_allows(a.role,a.company_id,$5)",
          [
            job.assignment_id,
            job.created_by,
            job.company_id,
            Date.now(),
            job.approval_mode === "direct"
              ? "iam.directory.apply"
              : "iam.directory.manage",
          ],
        )
      ).rowCount;
      if (
        !allowed ||
        !state?.write_enabled ||
        conn.version !== job.connection_version ||
        state.version !== job.state_version
      )
        fail(
          "DIRECTORY_AUTHORITY_CHANGED",
          "The authorizing role or connector configuration changed.",
        );
      const impact = await prepareDirectoryPreview(
        c,
        { companyId: job.company_id, userId: job.created_by } as IamActor,
        job.definition,
      );
      if (job.approval_mode === "direct") {
        await requireFreshMfa({
          userId: job.created_by,
          sessionId: job.mfa_session_id,
        });
        for (const companyId of impact.evidence.companies)
          if (
            !(
              await c.query(
                "SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) AND iam_role_allows(a.role,a.company_id,'iam.directory.apply')",
                [companyId, job.created_by, Date.now()],
              )
            ).rowCount
          )
            fail(
              "DIRECTORY_APPLY_SCOPE",
              "The operator's Company authority changed.",
              403,
            );
      } else {
        const review = (
          await c.query(
            "SELECT reviewed_by FROM iam_reviews WHERE id=$1 AND status='approved'",
            [job.review_id],
          )
        ).rows[0];
        for (const companyId of impact.evidence.companies)
          if (
            !review ||
            !(
              await c.query(
                "SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND iam_assignment_active(a.id,$3) AND iam_role_allows(a.role,a.company_id,'iam.review') AND iam_role_allows(a.role,a.company_id,'iam.directory.manage')",
                [companyId, review.reviewed_by, Date.now()],
              )
            ).rowCount
          )
            fail(
              "DIRECTORY_REVIEW_AUTHORITY_CHANGED",
              "The directory reviewer's authority changed.",
            );
      }
      if (
        hash(impact.evidence.identityLinks) !==
          hash(job.evidence.identityLinks) ||
        hash(impact.evidence.companies) !== hash(job.evidence.companies)
      )
        fail(
          "DIRECTORY_IDENTITY_CHANGED",
          "Account links or affected Companies changed after review.",
        );
      await c.query(
        "UPDATE people_directory_operations SET status='running',attempts=attempts+1,version=version+1,updated_at=$1 WHERE id=$2",
        [Date.now(), job.id],
      );
      const actions: DirectoryAction[] = [
        job.definition.action,
        ...(job.definition.additionalActions ?? []),
      ];
      const steps: {
        index: number;
        status: string;
        result: Record<string, unknown>;
      }[] = job.steps ?? [];
      for (const [index, action] of actions.entries()) {
        if (steps.some((s) => s.index === index && s.status === "completed"))
          continue;
        const result = await applyDirectoryAction(
          conn,
          actions.length === 1 ? job.id : `${job.id}:${index}`,
          action,
          job.evidence.inventory,
        );
        if (result.verified !== true)
          fail(
            "DIRECTORY_READBACK_FAILED",
            "The provider has not verified this operation.",
          );
        steps.push({ index, status: "completed", result });
        await c.query(
          "UPDATE people_directory_operations SET steps=$2 WHERE id=$1",
          [job.id, JSON.stringify(steps)],
        );
      }
      const result = {
        ...steps.at(-1)?.result,
        verified: true,
        endpointConfirmed: false,
        steps,
      };
      await c.query("BEGIN");
      await c.query(
        "UPDATE people_directory_operations SET status='completed',result=$1,last_error=NULL,version=version+1,updated_at=$2 WHERE id=$3",
        [JSON.stringify(result), Date.now(), job.id],
      );
      if (job.provision_id) {
        const provision = (
          await c.query(
            "SELECT evidence FROM people_provision_operations WHERE id=$1",
            [job.provision_id],
          )
        ).rows[0];
        for (const action of actions)
          if (action.kind === "group.members" && action.action === "add") {
            for (const plan of provision.evidence.plans)
              if (
                plan.definition.connectionId === conn.id &&
                plan.definition.directoryGroupIds.includes(action.objectId)
              ) {
                for (const member of action.memberIds.filter((id) =>
                  plan.directoryMembers.includes(id),
                )) {
                  const applicationId = job.provision_id + ":" + plan.id;
                  await c.query(
                    "INSERT INTO people_directory_membership_sources VALUES($1,$2,$3,$4,$4,$5) ON CONFLICT DO NOTHING",
                    [
                      conn.id,
                      action.objectId,
                      member,
                      applicationId,
                      Date.now(),
                    ],
                  );
                }
              }
          }
      }
      if (!job.provision_id)
        for (const action of actions)
          if (action.kind === "group.members" && action.action === "add") {
            for (const member of action.memberIds)
              await c.query(
                "INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
                [
                  conn.id,
                  action.objectId,
                  member,
                  "direct:" + job.id,
                  Date.now(),
                ],
              );
          }
      await c.query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [
        randomUUID(),
        job.company_id,
        job.created_by,
        job.assignment_id,
        "directory.operation.verified",
        job.id,
        JSON.stringify({ result }),
        Date.now(),
      ]);
      await c.query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$1,'people.directory.verified','people_directory',$2,1,$3,$4) ON CONFLICT(id) DO NOTHING",
        [
          "directory-verified:" + job.id,
          job.id,
          JSON.stringify({ id: job.id, companyId: job.company_id, result }),
          Date.now(),
        ],
      );
      await c.query("COMMIT");
    } catch (error) {
      await c.query("ROLLBACK");
      const code =
          error instanceof ApiError ? error.code : "DIRECTORY_UNAVAILABLE",
        retry = [
          "DIRECTORY_UNAVAILABLE",
          "DIRECTORY_REQUEST_FAILED",
          "DIRECTORY_READBACK_FAILED",
        ].includes(code);
      await c.query(
        "UPDATE people_directory_operations SET status=$1,last_error=$2,next_attempt_at=$3+LEAST(300000,1000*power(2,LEAST(attempts,8))),version=version+1,updated_at=$3 WHERE id=$4 AND status NOT IN ('completed','cancelled','rejected')",
        [retry ? "retry" : "blocked", code, Date.now(), row.id],
      );
    } finally {
      if (locked)
        await c.query("SELECT pg_advisory_unlock(hashtext($1))", [
          "people-directory:" + row.connection_id,
        ]);
      c.release();
    }
  }
}
