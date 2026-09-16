import "server-only";
import { createHash } from "node:crypto";
import type { PoolClient } from "pg";
import type { PostgresDatabase } from "./runtime/postgres";
import type { IamActor } from "./iam-context";
import { iamDb } from "./iam-db";
import { iamCommand } from "./iam-transactions";
import { requirePermission } from "./iam-permissions";
import { requireFreshMfa } from "./iam-mfa";
import { hasPermission } from "@/lib/permissions";
import { ApiError } from "./http";
import { grantInput, insertIamGrant, validateIamGrant } from "./iam-service";
import {
  managementChange,
  personRuleFields,
  ruleMatches,
  validateAttributeValue as validateValue,
  type ManagementChange,
  type ManagementSnapshot,
  type AttributeDefinition,
  type LocalGroup,
  type AccessPolicy,
  type PolicyAttachment,
  type ManagementImpact,
} from "@/lib/user-management";

function validateAttributeValue(field: AttributeDefinition, value: unknown) {
  try {
    return validateValue(field, value);
  } catch (error) {
    throw new ApiError(
      422,
      "ATTRIBUTE_INVALID",
      error instanceof Error ? error.message : "Check the attribute value.",
    );
  }
}

function fail(
  message: string,
  code = "MANAGEMENT_CHANGED",
  status = 409,
): never {
  throw new ApiError(status, code, message);
}
const allowedScope = async (
  c: PoolClient,
  actor: IamActor,
  node: string | null,
) => {
  if (actor.platform)
    fail(
      "User management is available in Company workspaces.",
      "SCOPE_FORBIDDEN",
      403,
    );
  if (
    node &&
    !(
      await c.query(
        "SELECT 1 FROM organization_nodes WHERE company_id=$1 AND id=$2 AND status='active'",
        [actor.companyId, node],
      )
    ).rowCount
  )
    fail("Choose an active unit in this Company.");
  if (
    actor.scopeKind !== "holding" &&
    (!node ||
      !(
        await c.query("SELECT um_descendant($1,$2,$3) ok", [
          actor.companyId,
          node,
          actor.scopeNodeId,
        ])
      ).rows[0].ok)
  )
    fail("This object is outside your workspace.", "SCOPE_FORBIDDEN", 403);
};
const visiblePerson = async (c: PoolClient, actor: IamActor, id: string) => {
  const p = (
    await c.query(
      "SELECT * FROM hr_people WHERE id=$1 AND company_id=$2 AND merged_into_id IS NULL",
      [id, actor.companyId],
    )
  ).rows[0];
  if (
    !p ||
    (actor.scopeKind !== "holding" &&
      !(
        await c.query("SELECT um_person_in_unit($1,$2,$3) ok", [
          actor.companyId,
          id,
          actor.scopeNodeId,
        ])
      ).rows[0].ok)
  )
    fail("This person is outside your workspace.", "SCOPE_FORBIDDEN", 403);
  return p;
};
async function groupRow(c: PoolClient, actor: IamActor, id: string) {
  const row = (
    await c.query("SELECT * FROM um_groups WHERE company_id=$1 AND id=$2", [
      actor.companyId,
      id,
    ])
  ).rows[0];
  if (!row) fail("Choose a local group.");
  await allowedScope(c, actor, row.definition.scopeNodeId);
  return row;
}
function commandPermission(change: ManagementChange) {
  return change.operation === "attribute"
    ? "iam.roles.manage"
    : change.operation === "values"
      ? change.input.target === "person"
        ? "people.manage"
        : change.input.target === "unit"
          ? "organization.manage"
          : "iam.manage"
      : "iam.manage";
}

export async function managementSnapshot(
  c: PoolClient,
  actor: IamActor,
): Promise<ManagementSnapshot> {
  requirePermission(actor, "iam.read");
  if (actor.platform)
    fail("Choose a Company workspace.", "SCOPE_FORBIDDEN", 403);
  const company = actor.companyId,
    scope = actor.scopeKind === "holding" ? null : actor.scopeNodeId;
  const groups = (
    await c.query(
      "SELECT g.id,g.version,g.definition FROM um_groups g WHERE company_id=$1 AND ($2::text IS NULL OR g.definition->>'scopeNodeId' IS NOT NULL AND um_descendant($1,g.definition->>'scopeNodeId',$2)) ORDER BY g.definition->>'name',g.id",
      [company, scope],
    )
  ).rows.map(
    (r) =>
      ({
        ...r.definition,
        id: r.id,
        version: r.version,
        members: [],
      }) as LocalGroup,
  );
  const people = (
    await c.query(
      "SELECT p.id,p.legal_name name,p.user_id,um_person_facts($1,p.id) definition,COALESCE((SELECT jsonb_agg(DISTINCT COALESCE(e.department_id,e.branch_id)) FROM hr_engagements e WHERE e.company_id=$1 AND e.person_id=p.id AND e.status='active'),'[]'::jsonb) groups_json FROM hr_people p WHERE p.company_id=$1 AND p.merged_into_id IS NULL AND ($2::text IS NULL OR um_person_in_unit($1,p.id,$2)) ORDER BY p.legal_name,p.id",
      [company, scope],
    )
  ).rows.map((r) => ({
    id: r.id,
    name: r.name,
    userId: r.user_id,
    facts: r.definition,
    nodes: r.groups_json.filter(Boolean),
  }));
  const memberships = (
    await c.query(
      "SELECT g.id,p.id person_id FROM um_groups g CROSS JOIN hr_people p WHERE g.company_id=$1 AND p.company_id=$1 AND um_group_member($1,p.id,g.id)",
      [company],
    )
  ).rows;
  for (const g of groups)
    g.members = memberships
      .filter((m) => m.id === g.id && people.some((p) => p.id === m.person_id))
      .map((m) => m.person_id);
  const nodes = (
    await c.query(
      "SELECT id,parent_id,display_name,kind FROM organization_nodes WHERE company_id=$1 AND status='active' AND ($2::text IS NULL OR um_descendant($1,id,$2)) ORDER BY display_name,id",
      [company, scope],
    )
  ).rows.map((r) => ({
    id: r.id,
    parentId: r.parent_id,
    label: r.display_name,
    kind: r.kind,
  }));
  const attachments = (
    await c.query(
      "SELECT * FROM um_attachments WHERE company_id=$1 ORDER BY id",
      [company],
    )
  ).rows
    .filter((r) =>
      r.definition.target === "group"
        ? groups.some((g) => g.id === r.definition.targetId)
        : nodes.some((n) => n.id === r.definition.targetId),
    )
    .map(
      (r) =>
        ({ ...r.definition, id: r.id, version: r.version }) as PolicyAttachment,
    );
  const policies = (
    await c.query(
      "SELECT * FROM um_policies WHERE company_id=$1 ORDER BY definition->>'name',id",
      [company],
    )
  ).rows
    .filter(
      (r) =>
        actor.scopeKind === "holding" ||
        r.definition.grants.every((g: { scopeNodeId: string | null }) =>
          nodes.some((n) => n.id === g.scopeNodeId),
        ),
    )
    .map(
      (r) =>
        ({ ...r.definition, id: r.id, version: r.version }) as AccessPolicy,
    );
  const attributes = (
    await c.query(
      "SELECT * FROM um_attributes WHERE company_id=$1 ORDER BY definition->>'label',id",
      [company],
    )
  ).rows.map(
    (r) =>
      ({
        ...r.definition,
        id: r.id,
        version: r.version,
      }) as AttributeDefinition,
  );
  const values = (
    await c.query("SELECT * FROM um_values WHERE company_id=$1", [company])
  ).rows
    .filter((r) =>
      r.target === "person"
        ? people.some((p) => p.id === r.object_id)
        : r.target === "group"
          ? groups.some((g) => g.id === r.object_id)
          : nodes.some((n) => n.id === r.object_id),
    )
    .map((r) => ({
      target: r.target,
      objectId: r.object_id,
      values: r.definition,
      version: r.version,
      expectedVersion: r.version,
      reason: "",
    }));
  const candidates = (
    await c.query(
      "SELECT c.*,r.status review_status,r.version review_version,CASE WHEN c.status='active' THEN iam_assignment_active(c.assignment_id,$2) ELSE true END enabled FROM um_candidates c LEFT JOIN iam_reviews r ON r.id=c.review_id WHERE c.company_id=$1 ORDER BY c.created_at DESC,c.id",
      [company, Date.now()],
    )
  ).rows
    .filter(
      (r) =>
        people.some((p) => p.id === r.person_id) &&
        attachments.some((a) => a.id === r.attachment_id),
    )
    .map((r) => {
      const a = attachments.find((a) => a.id === r.attachment_id)!,
        p = policies.find((p) => p.id === a.policyId);
      const status =
        r.review_status === "rejected"
          ? "rejected"
          : r.status === "active" && r.definition.validUntil <= Date.now()
            ? "expired"
            : r.status === "active" && !r.enabled
              ? "revoked"
              : r.status;
      return {
        id: r.id,
        personId: r.person_id,
        userId: r.user_id,
        attachmentId: r.attachment_id,
        policyName: p?.name ?? "Access policy",
        roleId: r.definition.roleId,
        status,
        reviewId: r.review_id,
        reviewVersion: r.review_version ?? 0,
        assignmentId: r.assignment_id,
        validUntil: r.definition.validUntil,
        explanation: `${a.target === "group" ? "Group" : "Organization"}: ${a.target === "group" ? groups.find((g) => g.id === a.targetId)?.name : nodes.find((n) => n.id === a.targetId)?.label} · ${status === "pending" ? "Independent review required" : (r.definition.blockedReason ?? status)}`,
      };
    });
  return {
    attributes,
    groups,
    policies,
    attachments,
    values,
    people,
    nodes,
    candidates,
    canManage: hasPermission(actor, "iam.manage"),
    canDefine:
      actor.scopeKind === "holding" && hasPermission(actor, "iam.roles.manage"),
    canManagePeople: hasPermission(actor, "people.manage"),
    canManageOrganization: hasPermission(actor, "organization.manage"),
    canReview: hasPermission(actor, "iam.review"),
  };
}
export async function readManagement(db: PostgresDatabase, actor: IamActor) {
  const c = await iamDb(db).pool.connect();
  try {
    await c.query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY");
    const result = await managementSnapshot(c, actor);
    await c.query("COMMIT");
    return result;
  } finally {
    await c.query("ROLLBACK");
    c.release();
  }
}
async function save(
  c: PoolClient,
  actor: IamActor,
  table: string,
  input: { id?: string; expectedVersion: number },
  definition: unknown,
  extra?: { authorId: string; parentId: string },
) {
  const id = input.id ?? crypto.randomUUID(),
    old = (
      await c.query(
        `SELECT version FROM ${table} WHERE company_id=$1 AND id=$2 FOR UPDATE`,
        [actor.companyId, id],
      )
    ).rows[0];
  if ((old?.version ?? 0) !== input.expectedVersion || (input.id && !old))
    fail("This item changed. Refresh and preview again.");
  const version = input.expectedVersion + 1;
  if (extra)
    await c.query(
      `INSERT INTO ${table}(company_id,id,version,definition,author_id,parent_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(company_id,id) DO UPDATE SET version=excluded.version,definition=excluded.definition,author_id=excluded.author_id,parent_id=excluded.parent_id`,
      [
        actor.companyId,
        id,
        version,
        JSON.stringify(definition),
        extra.authorId,
        extra.parentId,
      ],
    );
  else
    await c.query(
      `INSERT INTO ${table}(company_id,id,version,definition) VALUES($1,$2,$3,$4) ON CONFLICT(company_id,id) DO UPDATE SET version=excluded.version,definition=excluded.definition`,
      [actor.companyId, id, version, JSON.stringify(definition)],
    );
  await c.query(
    "INSERT INTO um_history(company_id,entity,object_id,version,definition,created_at) VALUES($1,$2,$3,$4,$5,$6)",
    [
      actor.companyId,
      table,
      id,
      version,
      JSON.stringify(definition),
      Date.now(),
    ],
  );
  return id;
}
async function validatePolicy(
  c: PoolClient,
  actor: IamActor,
  policy: Pick<AccessPolicy, "grants">,
) {
  const parent = (
    await c.query("SELECT valid_until FROM assignments WHERE id=$1", [
      actor.assignmentId,
    ])
  ).rows[0];
  for (const grant of policy.grants) {
    await allowedScope(c, actor, grant.scopeNodeId);
    if (
      (grant.scopeKind === "holding" && actor.scopeKind !== "holding") ||
      (grant.scopeKind !== "holding" && !grant.scopeNodeId)
    )
      fail("Choose a supported grant scope.");
    if (
      grant.scopeKind !== "holding" &&
      !(
        await c.query(
          "SELECT 1 FROM organization_nodes WHERE company_id=$1 AND id=$2 AND kind=$3",
          [actor.companyId, grant.scopeNodeId, grant.scopeKind],
        )
      ).rowCount
    )
      fail("Grant scope and unit must match.");
    if (
      grant.validUntil <= Date.now() ||
      grant.validUntil >
        Math.min(Number(parent.valid_until), Date.now() + 90 * 86400000)
    )
      fail("Choose an expiry within 90 days and your assignment validity.");
    const role = (
      await c.query(
        "SELECT v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.published_revision=$3 AND r.status='active'",
        [actor.companyId, grant.roleId, grant.expectedRoleRevision],
      )
    ).rows[0];
    if (!role || !role.scopes_json.includes(grant.scopeKind))
      fail("The role changed or does not support this scope.");
    if (
      [...role.permissions_json, ...role.grantable_json].some(
        (p: string) => !actor.iam?.grantablePermissions.includes(p),
      )
    )
      fail(
        "The role exceeds your delegated permissions.",
        "DELEGATION_EXCEEDED",
        403,
      );
    if (
      role.permissions_json.includes("company.owner") ||
      role.permissions_json.includes("iam.recovery")
    )
      fail(
        "Ownership and recovery access must use individually reviewed direct assignments.",
      );
  }
}
async function applyChange(
  c: PoolClient,
  actor: IamActor,
  change: ManagementChange,
) {
  requirePermission(actor, commandPermission(change));
  const snapshot = await managementSnapshot(c, actor);
  if (change.operation === "attribute") {
    if (actor.scopeKind !== "holding")
      fail(
        "Manage attribute definitions in Company scope.",
        "SCOPE_FORBIDDEN",
        403,
      );
    const input = change.input,
      old = snapshot.attributes.find((a) => a.id === input.id);
    if (old && (old.type !== input.type || old.target !== input.target))
      fail("Field type and target are permanent. Create a new field instead.");
    const used = (
      await c.query(
        "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean AND EXISTS(SELECT 1 FROM jsonb_array_elements(definition->'rule'->'conditions') x WHERE x->>'field'=$2)",
        [actor.companyId, `custom:${input.id}`],
      )
    ).rowCount;
    if (
      used &&
      (input.archived ||
        (old && JSON.stringify(old.options) !== JSON.stringify(input.options)))
    )
      fail(
        "Update active group rules before archiving this field or changing its choices.",
      );
    if (old && old.options.some((o) => !input.options.includes(o)))
      fail(
        "Existing choices are retained for history. Add choices or create a replacement field.",
      );
    return save(c, actor, "um_attributes", input, input);
  }
  if (change.operation === "group") {
    const input = change.input;
    if (input.id) await groupRow(c, actor, input.id);
    await allowedScope(c, actor, input.scopeNodeId);
    let parent = input.parentId;
    const seen = new Set([input.id]);
    while (parent) {
      if (seen.has(parent) || seen.size > 32)
        fail("Groups cannot contain themselves or form a cycle.");
      seen.add(parent);
      const row = await groupRow(c, actor, parent);
      if (row.definition.archived || row.definition.kind !== "manual")
        fail("Choose an active manual parent group.");
      if (
        row.definition.scopeNodeId &&
        (!input.scopeNodeId ||
          !(
            await c.query("SELECT um_descendant($1,$2,$3) ok", [
              actor.companyId,
              input.scopeNodeId,
              row.definition.scopeNodeId,
            ])
          ).rows[0].ok)
      )
        fail("A child group must stay within its parent's workspace.");
      parent = row.definition.parentId;
    }
    const children = snapshot.groups.filter((g) => g.parentId === input.id);
    for (const child of children.filter((g) => !g.archived)) {
      if (
        input.scopeNodeId &&
        (!child.scopeNodeId ||
          !(
            await c.query("SELECT um_descendant($1,$2,$3) ok", [
              actor.companyId,
              child.scopeNodeId,
              input.scopeNodeId,
            ])
          ).rows[0].ok)
      )
        fail("Move child groups before narrowing this group's workspace.");
    }
    if (
      input.id &&
      (input.archived || input.kind === "automatic") &&
      children.some((g) => !g.archived)
    )
      fail(
        "Move child groups before archiving or making this group automatic.",
      );
    if (input.rule)
      for (const condition of input.rule.conditions) {
        const custom = snapshot.attributes.find(
            (a) =>
              `custom:${a.id}` === condition.field &&
              a.target === "person" &&
              !a.archived,
          ),
          builtin = personRuleFields.find((f) => f.id === condition.field),
          type = custom?.type ?? builtin?.type;
        if (!type) fail("Choose an active person field for each rule.");
        if (!["empty", "not-empty"].includes(condition.op)) {
          if (condition.value === undefined)
            fail("Each comparison needs a value.");
          if (
            ["gt", "gte", "lt", "lte"].includes(condition.op) &&
            !["number", "date"].includes(type)
          )
            fail("Ordered comparisons require a number or date field.");
          if ((condition.op === "contains") !== (type === "multi-select"))
            fail(
              "Multi-select fields use contains; other fields use comparisons.",
            );
          if (custom)
            validateAttributeValue(
              {
                ...custom,
                type:
                  custom.type === "multi-select"
                    ? "single-select"
                    : custom.type,
              },
              condition.value,
            );
          else if (typeof condition.value !== "string")
            fail("Choose a text value for this field.");
        }
      }
    return save(c, actor, "um_groups", input, input);
  }
  if (change.operation === "membership") {
    const input = change.input,
      row = await groupRow(c, actor, input.groupId);
    if (row.version !== input.expectedVersion)
      fail("The group changed. Refresh and preview again.");
    if (row.definition.kind !== "manual" || row.definition.archived)
      fail("Automatic groups use rules. Choose an active manual group.");
    for (const personId of input.personIds) {
      await visiblePerson(c, actor, personId);
      if (
        !(
          await c.query("SELECT um_person_in_unit($1,$2,$3) ok", [
            actor.companyId,
            personId,
            row.definition.scopeNodeId,
          ])
        ).rows[0].ok
      )
        fail("A person is outside the group's workspace.");
      if (input.action === "add")
        await c.query(
          "INSERT INTO um_memberships VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
          [actor.companyId, input.groupId, personId],
        );
      else
        await c.query(
          "DELETE FROM um_memberships WHERE company_id=$1 AND group_id=$2 AND person_id=$3",
          [actor.companyId, input.groupId, personId],
        );
    }
    return save(
      c,
      actor,
      "um_groups",
      { id: input.groupId, expectedVersion: input.expectedVersion },
      row.definition,
    );
  }
  if (change.operation === "values") {
    const input = change.input;
    if (input.target === "person")
      await visiblePerson(c, actor, input.objectId);
    else if (input.target === "group") await groupRow(c, actor, input.objectId);
    else await allowedScope(c, actor, input.objectId);
    const old = (
      await c.query(
        "SELECT * FROM um_values WHERE company_id=$1 AND target=$2 AND object_id=$3 FOR UPDATE",
        [actor.companyId, input.target, input.objectId],
      )
    ).rows[0];
    if ((old?.version ?? 0) !== input.expectedVersion)
      fail("These values changed. Refresh first.");
    const values = { ...(old?.definition ?? {}) };
    for (const id of Object.keys(input.values))
      if (
        !snapshot.attributes.some(
          (a) => a.id === id && a.target === input.target && !a.archived,
        )
      )
        fail("An attribute is unavailable.");
    for (const field of snapshot.attributes.filter(
      (a) => a.target === input.target && !a.archived,
    ))
      values[field.id] = validateAttributeValue(
        field,
        Object.hasOwn(input.values, field.id)
          ? input.values[field.id]
          : values[field.id],
      );
    const version = input.expectedVersion + 1;
    await c.query(
      "INSERT INTO um_values VALUES($1,$2,$3,$4,$5) ON CONFLICT(company_id,target,object_id) DO UPDATE SET version=excluded.version,definition=excluded.definition",
      [
        actor.companyId,
        input.target,
        input.objectId,
        version,
        JSON.stringify(values),
      ],
    );
    await c.query(
      "INSERT INTO um_history(company_id,entity,object_id,version,definition,created_at) VALUES($1,$2,$3,$4,$5,$6)",
      [
        actor.companyId,
        `values:${input.target}`,
        input.objectId,
        version,
        JSON.stringify(values),
        Date.now(),
      ],
    );
    return input.objectId;
  }
  if (change.operation === "policy") {
    if (
      change.input.id &&
      !snapshot.policies.some((p) => p.id === change.input.id)
    )
      fail("The policy is outside your workspace.", "SCOPE_FORBIDDEN", 403);
    if (!change.input.archived) await validatePolicy(c, actor, change.input);
    return save(c, actor, "um_policies", change.input, change.input);
  }
  const input = change.input;
  if (input.id && !snapshot.attachments.some((a) => a.id === input.id))
    fail("The attachment is outside your workspace.", "SCOPE_FORBIDDEN", 403);
  const policy = snapshot.policies.find(
    (p) => p.id === input.policyId && (!p.archived || input.archived),
  );
  if (!policy) fail("Choose an active policy within your workspace.");
  if (!input.archived) await validatePolicy(c, actor, policy);
  if (input.target === "group") {
    const group = await groupRow(c, actor, input.targetId);
    if (group.definition.archived) fail("Choose an active group.");
  } else await allowedScope(c, actor, input.targetId);
  for (const excluded of input.exclusions) {
    if (input.target === "unit") {
      await allowedScope(c, actor, excluded);
      if (
        !(
          await c.query("SELECT um_descendant($1,$2,$3) ok", [
            actor.companyId,
            excluded,
            input.targetId,
          ])
        ).rows[0].ok
      )
        fail("Exclude only units beneath the target.");
    } else {
      let current: string | null = excluded,
        found = false;
      const seen = new Set<string>();
      while (current && !seen.has(current)) {
        seen.add(current);
        const row = await groupRow(c, actor, current);
        if (current === input.targetId) {
          found = true;
          break;
        }
        current = row.definition.parentId;
      }
      if (!found) fail("Exclude only groups beneath the target.");
    }
  }
  return save(c, actor, "um_attachments", input, input, {
    authorId: actor.userId,
    parentId: actor.assignmentId,
  });
}

/** Reconciliation only proposes access. It cannot approve or insert a grant. */
export async function reconcileManagement(c: PoolClient, companyId: string) {
  await c.query("SELECT pg_advisory_xact_lock(hashtext($1))", [
    `user-management:${companyId}`,
  ]);
  // An explicit revocation must not become a new proposal on every reconciliation.
  await c.query(
    "UPDATE um_candidates c SET status='rejected' FROM assignments a WHERE c.company_id=$1 AND c.assignment_id=a.id AND c.status='active' AND a.revoked_at IS NOT NULL AND um_attachment_matches(c.company_id,c.person_id,c.attachment_id)",
    [companyId],
  );
  await c.query("SELECT um_invalidate($1)", [companyId]);
  await c.query(
    "UPDATE um_candidates c SET status='rejected' FROM iam_reviews r WHERE c.review_id=r.id AND c.company_id=$1 AND c.status='pending' AND r.status='rejected'",
    [companyId],
  );
  const matches = (
    await c.query(
      "SELECT a.*,p.version policy_version,p.definition policy_data,h.id person_id,h.user_id FROM um_attachments a JOIN um_policies p ON p.company_id=a.company_id AND p.id=a.definition->>'policyId' CROSS JOIN hr_people h WHERE a.company_id=$1 AND h.company_id=$1 AND h.user_id IS NOT NULL AND h.status='active' AND um_attachment_matches($1,h.id,a.id) AND iam_assignment_active(a.parent_id,$2) ORDER BY a.id,h.id",
      [companyId, Date.now()],
    )
  ).rows;
  for (const match of matches) {
    const parent = (
      await c.query(
        "SELECT a.*,v.grantable_json FROM assignments a JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) JOIN iam_role_revisions v ON v.role_id=r.id AND v.company_id=r.company_id AND v.revision=r.published_revision WHERE a.id=$1 AND a.user_id=$2 AND iam_role_allows(a.role,a.company_id,'iam.manage')",
        [match.parent_id, match.author_id],
      )
    ).rows[0];
    if (!parent) continue;
    const proposer = {
      companyId,
      userId: match.author_id,
      assignmentId: parent.id,
      scopeKind: parent.scope_kind,
      scopeNodeId: parent.scope_node_id,
      platform: false,
      role: parent.role,
      iam: { grantablePermissions: parent.grantable_json },
    } as IamActor;
    // The legacy adapter deliberately serializes unknown JSON columns.
    const policy =
      typeof match.policy_data === "string"
        ? JSON.parse(match.policy_data)
        : match.policy_data;
    for (const [index, spec] of policy.grants.entries()) {
      // Fixed validity is never renewed by reconciliation.
      if (spec.validUntil <= Date.now()) continue;
      if (
        (
          await c.query(
            "SELECT 1 FROM um_candidates WHERE company_id=$1 AND person_id=$2 AND attachment_id=$3 AND grant_index=$4 AND attachment_version=$5 AND policy_version=$6 AND status IN ('pending','active','rejected','blocked')",
            [
              companyId,
              match.person_id,
              match.id,
              index,
              match.version,
              match.policy_version,
            ],
          )
        ).rowCount
      )
        continue;
      const grant = grantInput.parse({
        ...spec,
        userId: match.user_id,
        reason: `Policy: ${policy.name}`,
      });
      let blocked = "";
      try {
        await validateIamGrant(c, proposer, grant, match.parent_id);
      } catch (error) {
        if (!(error instanceof ApiError)) throw error;
        blocked = error.message;
      }
      if (blocked) {
        await c.query(
          "INSERT INTO um_candidates(id,company_id,person_id,user_id,attachment_id,attachment_version,policy_version,grant_index,definition,status,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'blocked',$10)",
          [
            crypto.randomUUID(),
            companyId,
            match.person_id,
            match.user_id,
            match.id,
            match.version,
            match.policy_version,
            index,
            JSON.stringify({ ...grant, blockedReason: blocked }),
            Date.now(),
          ],
        );
        continue;
      }
      const id = crypto.randomUUID(),
        reviewId = crypto.randomUUID();
      const payload = {
        ...grant,
        candidateId: id,
        parentId: match.parent_id,
        authorizingAssignmentId: match.parent_id,
      };
      await c.query(
        "INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,reason) VALUES($1,$2,'policy.grant',$3,$4,COALESCE((SELECT person_id FROM iam_members WHERE company_id=$2 AND user_id=$4),$4),$5,$6)",
        [
          reviewId,
          companyId,
          JSON.stringify(payload),
          match.author_id,
          Date.now(),
          grant.reason,
        ],
      );
      await c.query(
        "INSERT INTO um_candidates(id,company_id,person_id,user_id,attachment_id,attachment_version,policy_version,grant_index,definition,review_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        [
          id,
          companyId,
          match.person_id,
          match.user_id,
          match.id,
          match.version,
          match.policy_version,
          index,
          JSON.stringify(grant),
          reviewId,
          Date.now(),
        ],
      );
      await c.query(
        "INSERT INTO iam_audit VALUES($1,$2,$3,$4,'policy.grant.proposed',$5,$6,$7)",
        [
          crypto.randomUUID(),
          companyId,
          match.author_id,
          match.parent_id,
          id,
          JSON.stringify(payload),
          Date.now(),
        ],
      );
    }
  }
}
export async function applyManagementReview(
  c: PoolClient,
  actor: IamActor,
  p: Record<string, unknown>,
) {
  if (actor.platform) fail("Company policy required.");
  const row = (
    await c.query(
      "SELECT * FROM um_candidates WHERE company_id=$1 AND id=$2 FOR UPDATE",
      [actor.companyId, p.candidateId],
    )
  ).rows[0];
  if (
    !row ||
    row.status !== "pending" ||
    !(
      await c.query("SELECT um_candidate_eligible($1,$2) ok", [
        row.id,
        Date.now(),
      ])
    ).rows[0].ok
  )
    fail("Eligibility changed. Refresh the proposal.");
  const grant = grantInput.parse(row.definition);
  await validateIamGrant(c, actor, grant, String(p.parentId));
  const assignmentId = await insertIamGrant(
    c,
    actor,
    grant,
    String(p.parentId),
    "policy",
    row.id,
  );
  await c.query(
    "UPDATE um_candidates SET assignment_id=$1,status='active' WHERE id=$2",
    [assignmentId, row.id],
  );
}
const fingerprint = (change: ManagementChange, snapshot: ManagementSnapshot) =>
  createHash("sha256")
    .update(JSON.stringify({ change, snapshot }))
    .digest("hex");
function impact(
  before: ManagementSnapshot,
  after: ManagementSnapshot,
): ManagementImpact {
  const memberships = (s: ManagementSnapshot) =>
    s.groups.flatMap((g) =>
      g.members.map(
        (p) => `${s.people.find((v) => v.id === p)?.name ?? p} → ${g.name}`,
      ),
    );
  const old = memberships(before),
    next = memberships(after);
  return {
    membersAdded: next.filter((v) => !old.includes(v)),
    membersRemoved: old.filter((v) => !next.includes(v)),
    accessRemoved: before.candidates
      .filter(
        (v) =>
          ["active", "pending"].includes(v.status) &&
          !after.candidates.some((n) => n.id === v.id && n.status === v.status),
      )
      .map(
        (v) =>
          `${before.people.find((p) => p.id === v.personId)?.name} · ${v.roleId}`,
      ),
    accessPending: after.candidates
      .filter(
        (v) =>
          v.status === "pending" &&
          !before.candidates.some((o) => o.id === v.id),
      )
      .map(
        (v) =>
          `${after.people.find((p) => p.id === v.personId)?.name} · ${v.roleId}`,
      ),
    accessBlocked: after.candidates
      .filter(
        (v) =>
          v.status === "blocked" &&
          !before.candidates.some((o) => o.id === v.id),
      )
      .map(
        (v) =>
          (after.people.find((p) => p.id === v.personId)?.name ?? "Person") +
          " · " +
          v.explanation,
      ),
    matchingPeople: [],
  };
}
export async function managementCommand(
  db: PostgresDatabase,
  actor: IamActor,
  raw: unknown,
  key: string,
  preview: boolean,
  token?: string,
) {
  const change = managementChange.parse(raw);
  requirePermission(actor, commandPermission(change));
  if (!preview && ["policy", "attachment"].includes(change.operation))
    await requireFreshMfa(actor);
  const run = async (c: PoolClient) => {
    await c.query("SELECT pg_advisory_xact_lock(hashtext($1))", [
      `user-management:${actor.companyId}`,
    ]);
    const before = await managementSnapshot(c, actor),
      stamp = fingerprint(change, before);
    if (!preview && stamp !== token)
      fail("The preview changed. Review the latest impact before saving.");
    const id = await applyChange(c, actor, change);
    await reconcileManagement(c, actor.companyId);
    const after = await managementSnapshot(c, actor),
      changes = impact(before, after);
    if (change.operation === "group")
      changes.matchingPeople =
        after.groups
          .find((g) => g.id === id)
          ?.members.map((id) => {
            const person = after.people.find((p) => p.id === id);
            const rule = change.input.rule;
            if (!person || !rule) return person?.name ?? id;
            const matched = rule.conditions
              .filter((condition) =>
                ruleMatches(
                  { mode: "all", conditions: [condition] },
                  person.facts,
                ),
              )
              .map(
                (condition) =>
                  `${after.attributes.find((a) => "custom:" + a.id === condition.field)?.label ?? personRuleFields.find((f) => f.id === condition.field)?.label ?? condition.field}: ${condition.op} ${String(condition.value ?? "")}`,
              );
            return `${person.name} — ${matched.join(" · ")}`;
          }) ?? [];
    return { id, previewToken: stamp, impact: changes };
  };
  if (!preview)
    return iamCommand(
      db,
      actor,
      key,
      `user-management.${change.operation}`,
      change,
      commandPermission(change),
      run,
    );
  const c = await iamDb(db).pool.connect();
  try {
    await c.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
    return await run(c);
  } finally {
    await c.query("ROLLBACK");
    c.release();
  }
}
export async function managementWork(db: PostgresDatabase) {
  const c = await iamDb(db).pool.connect();
  try {
    const companies = (
      await c.query("SELECT DISTINCT company_id FROM um_attachments")
    ).rows;
    for (const row of companies) {
      await c.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
      try {
        await reconcileManagement(c, row.company_id);
        await c.query("COMMIT");
      } catch (error) {
        await c.query("ROLLBACK");
        throw error;
      }
    }
  } finally {
    c.release();
  }
}
