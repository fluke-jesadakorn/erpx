import "server-only";
import { createHash, createHmac } from "node:crypto";
import type { PoolClient } from "pg";
import { peopleAccessChange, type PeopleAccessChange, type PeopleAccessSnapshot, type PeopleAccessPreview, type AccessSubject } from "@/lib/identity_access/people-access";
import { permissionCatalog, hasPermission } from "@/lib/identity_access/permissions";
import { ApiError } from "../../runtime/public/http";
import { transactionDatabase, type PostgresDatabase } from "../../runtime/public/runtime/postgres";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { runtimeEnv } from "../../runtime/public/runtime";
import { internalAccessPeople } from "../../people_payroll/public/iam-subject-facts";
import { accessUnitFacts } from "../../organization/public/iam-unit-facts";
import { stageInvitationPersonLinks } from "../../people_payroll/public/identity-person-link-effects";
import type { Invitation } from "../domain/account-access";
import { requireExternalReady, requireDelegatedRole, requireAccessDeadline } from "../domain/people-access";
import type { IamActor } from "../domain/iam-actor";
import { assertCompanyScope } from "../public/iam-context";
import { requirePermission } from "../public/iam-permissions";
import { requireFreshMfa } from "../public/iam-mfa";
import { iamCommand } from "../public/iam-transactions";
import { iamDb } from "../public/iam-db";
import { validateIamGrant, insertIamGrant, grantInput } from "../public/iam-service";

const hash = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const json = <T,>(value: T | string): T => typeof value === "string" ? JSON.parse(value) : value;
const unavailable = () => new ApiError(404, "ACCESS_OBJECT_UNAVAILABLE", "This object is unavailable in your selected Company.");
function authorize(actor: IamActor, write = false) {
  if (actor.platform) throw new ApiError(403, "COMPANY_SCOPE_REQUIRED", "Select a Company workspace for People access.");
  assertCompanyScope(actor);
  requirePermission(actor, write ? "iam.access.apply" : "iam.read");
}
async function snapshot(client: PoolClient, actor: IamActor): Promise<PeopleAccessSnapshot> {
  const db = transactionDatabase(client), company = actor.companyId;
  const [internal, units, organizations, teams, external, roles, assignments, parent, members, internalSubjects] = await Promise.all([
    internalAccessPeople(db, company), accessUnitFacts(db, company),
    client.query(`SELECT id,name,kind,version FROM iam_external_organizations WHERE company_id=$1 ORDER BY name,id`, [company]),
    client.query(`SELECT id,name,organization_id AS "organizationId",department_id AS "departmentId",version FROM iam_external_teams WHERE company_id=$1 ORDER BY name,id`, [company]),
    client.query(`SELECT id,name,email,kind,user_id AS "userId",person_id AS "personId",NULL::text AS "departmentId",organization_id AS "organizationId",team_id AS "teamId",sponsor_person_id AS "sponsorPersonId",expires_at AS "expiresAt",status,version FROM iam_access_subjects WHERE company_id=$1 AND kind='external' ORDER BY name,id`, [company]),
    client.query(`SELECT r.id,r.name,r.built_in AS "builtIn",r.published_revision AS revision,v.permissions_json AS permissions,v.grantable_json AS grantable,v.scopes_json AS scopes FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.status='active' ORDER BY r.name,r.id`, [company]),
    client.query(`SELECT id,user_id AS "userId",role AS "roleId",scope_kind AS "scopeKind",scope_node_id AS "scopeNodeId",valid_until AS "validUntil",version,iam_assignment_active(id,$2) AS active FROM assignments WHERE company_id=$1 AND revoked_at IS NULL ORDER BY user_id,id`, [company, Date.now()]),
    client.query(`SELECT valid_until FROM assignments WHERE id=$1 AND company_id=$2`, [actor.assignmentId, company]),
    client.query(`SELECT user_id,person_id FROM iam_members WHERE company_id=$1`, [company]),
    client.query(`SELECT id,status,user_id FROM iam_access_subjects WHERE company_id=$1 AND kind='internal'`, [company]),
  ]);
  const ceiling = actor.iam?.grantablePermissions ?? [];
  const availableRoles = roles.rows.map(r => ({ ...r, permissions:json<string[]>(r.permissions),grantable:json<string[]>(r.grantable),scopes:json<string[]>(r.scopes) }));
  const employees: AccessSubject[] = internal.map(p => ({ ...p, id: `employee:${p.personId}`, kind: "internal", organizationId: null, teamId: null, sponsorPersonId: null, expiresAt: null, version: 0, status: internalSubjects.rows.find(s => s.id === `employee:${p.personId}`)?.status ?? p.status }));
  const subjects: AccessSubject[] = [...employees, ...external.rows.map(p => ({ ...p, expiresAt: p.expiresAt == null ? null : Number(p.expiresAt) }))];
  const missingUsers = [...new Set([...assignments.rows.map(a => a.userId), ...members.rows.map(m => m.user_id)])].filter(userId => !subjects.some(s => s.userId === userId));
  if (missingUsers.length) {
    const env = await runtimeEnv();
    const accounts = (await env.CONTROL_DB.prepare(`SELECT id,name,email FROM "user" WHERE id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY name,id`).bind(JSON.stringify(missingUsers)).all<{id:string;name:string;email:string}>()).results;
    for (const account of accounts) {
      const personId = members.rows.find(m => m.user_id === account.id)?.person_id ?? null, employee = employees.find(p => p.personId === personId);
      subjects.push({ id: `account:${account.id}`, name: account.name, email: account.email, userId: account.id, personId, kind: employee ? "internal" : "external", departmentId: employee?.departmentId ?? null, organizationId: null, teamId: null, sponsorPersonId: null, expiresAt: null, status: "active", version: 0 });
    }
  }
  return {
    subjects,
    organizations: organizations.rows, teams: teams.rows, units,
    roles: availableRoles.filter(r => [...r.permissions, ...r.grantable].every(p => ceiling.includes(p)) && !r.permissions.includes("company.owner") && !r.permissions.includes("iam.recovery")).map(r => ({ id:r.id,name:r.name,builtIn:r.builtIn,revision:r.revision,permissions:r.permissions,grantablePermissions:r.grantable,scopes:r.scopes })),
    permissions: permissionCatalog.filter(p => p.availability === "live" && p.namespace === "customer" && ceiling.includes(p.id) && !["company.owner", "iam.recovery"].includes(p.id)).map(p => ({ id: p.id, label: p.label })),
    assignments: assignments.rows.map(a => ({ ...a, validUntil: Number(a.validUntil) })),
    canApply: hasPermission(actor, "iam.access.apply"), authorizingUntil: Number(parent.rows[0]?.valid_until ?? 0),
  };
}
export async function peopleAccessSnapshot(db: PostgresDatabase, actor: IamActor) {
  db = iamDb(db);
  authorize(actor);
  const client = await db.pool.connect();
  try { await client.query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY"); const result = await snapshot(client, actor); await client.query("COMMIT"); return result; }
  catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}
function findSubject(data: PeopleAccessSnapshot, id: string): AccessSubject {
  const subject = data.subjects.find(s => s.id === id); if (!subject) throw unavailable(); return subject;
}
async function validate(client: PoolClient, actor: IamActor, data: PeopleAccessSnapshot, change: PeopleAccessChange) {
  const now = Date.now();
  if (change.operation === "organization" || change.operation === "team" || change.operation === "subject") {
    const rows = change.operation === "organization" ? data.organizations : change.operation === "team" ? data.teams : data.subjects.filter(s => s.kind === "external");
    const old = change.id ? rows.find(r => r.id === change.id) : null;
    if (change.id && !old) throw unavailable();
    if (change.expectedVersion !== (old?.version ?? 0)) throw new ApiError(409, "ACCESS_OBJECT_CHANGED", "This record changed. Refresh and preview again.");
    if (change.operation !== "organization" && !data.organizations.some(o => o.id === change.organizationId)) throw unavailable();
    if (change.operation === "team" && change.departmentId && !data.units.some(u => u.id === change.departmentId && u.kind === "department")) throw unavailable();
    if (change.operation === "team" && old && change.organizationId !== data.teams.find(t=>t.id===change.id)?.organizationId && data.subjects.some(s=>s.teamId===change.id)) throw new ApiError(409,"TEAM_HAS_MEMBERS","Move this team's users before changing its external organization.");
    if (change.operation === "subject") {
      if (change.teamId && !data.teams.some(t => t.id === change.teamId && t.organizationId === change.organizationId)) throw new ApiError(422, "TEAM_ORGANIZATION_MISMATCH", "Choose a team from this external organization.");
      if (change.sponsorPersonId && !data.subjects.some(s => s.kind === "internal" && s.personId === change.sponsorPersonId && s.status === "active")) throw new ApiError(422, "SPONSOR_UNAVAILABLE", "Choose an active employee in this Company.");
      const subject = old as AccessSubject | null;
      if (subject?.userId && (await client.query("SELECT iam_same_person($1,$2,$3) same", [subject.userId, actor.userId, actor.companyId])).rows[0].same) throw new ApiError(403, "SELF_ACCESS_CHANGE", "Another administrator must change your access.");
      if (subject?.userId && change.status === "active") requireExternalReady({ ...subject, ...change, kind: "external" }, now);
      if (change.expiresAt != null && change.expiresAt <= now && change.status === "active") throw new ApiError(422, "EXPIRY_INVALID", "Choose a future relationship expiry.");
    }
  }
  if (change.operation === "role") {
    requirePermission(actor, "iam.roles.manage");
    requireDelegatedRole(change.permissions, actor.iam?.grantablePermissions ?? []);
    if (change.permissions.some(p => !data.permissions.some(d => d.id === p))) throw new ApiError(422, "PERMISSION_UNAVAILABLE", "Choose a published permission.");
    const old = change.id ? data.roles.find(r => r.id === change.id) : null;
    if (change.id && (!old || old.builtIn || !change.id.startsWith("custom_"))) throw new ApiError(403, "PROTECTED_ROLE", "Create a custom role instead of replacing a built-in role.");
    if (change.expectedVersion !== (old?.revision ?? 0)) throw new ApiError(409, "ROLE_CHANGED", "This role changed. Preview again.");
    if (change.id && (await client.query(`SELECT 1 FROM assignments WHERE company_id=$1 AND role=$2 AND revoked_at IS NULL AND iam_same_person(user_id,$3,$1) LIMIT 1`, [actor.companyId, change.id, actor.userId])).rowCount) throw new ApiError(403, "SELF_ACCESS_CHANGE", "Another administrator must publish a role assigned to you.");
  }
  if (change.operation === "grant" || change.operation === "invite") {
    const subject = findSubject(data, change.subjectId);
    requireAccessDeadline(change, subject, data.authorizingUntil, now);
    if (subject.status === "suspended" || subject.kind === "internal" && subject.status !== "active") throw new ApiError(422, "SUBJECT_INACTIVE", "Activate this relationship before enabling access.");
    if (change.operation === "grant" && !subject.userId) throw new ApiError(422, "INVITE_REQUIRED", "Invite this user first.");
    if (change.operation === "invite" && (subject.userId || !subject.email)) throw new ApiError(422, "INVITATION_UNAVAILABLE", "Choose an unlinked user with an email address.");
    if (change.scopeKind !== "holding" && !data.units.some(u => u.id === change.scopeNodeId && u.kind === change.scopeKind)) throw unavailable();
    if (change.scopeKind === "holding" && change.scopeNodeId !== null) throw new ApiError(422, "SCOPE_INVALID", "Holding access has no local scope target.");
    const role = data.roles.find(r => r.id === change.roleId); if (!role) throw unavailable();
    await validateIamGrant(client, actor, grantInput.parse({ ...change, userId: subject.userId ?? "usr_invite_pending", expectedRoleRevision: role.revision }));
  }
  if (change.operation === "revoke") {
    const assignment = data.assignments.find(a => a.id === change.assignmentId);
    if (!assignment) throw unavailable();
    if (!data.roles.some(r => r.id === assignment.roleId)) throw new ApiError(403, "DELEGATION_EXCEEDED", "The assignment exceeds your delegated authority.");
    if (assignment.version !== change.expectedVersion) throw new ApiError(409, "ASSIGNMENT_CHANGED", "This assignment changed. Preview again.");
    if ((await client.query("SELECT iam_same_person($1,$2,$3) same", [assignment.userId, actor.userId, actor.companyId])).rows[0].same) throw new ApiError(403, "SELF_ACCESS_CHANGE", "Another administrator must change your access.");
  }
}
export async function previewPeopleAccess(db: PostgresDatabase, actor: IamActor, input: unknown): Promise<PeopleAccessPreview> {
  db = iamDb(db);
  authorize(actor, true); await requireFreshMfa(actor);
  const change = peopleAccessChange.parse(input), token = crypto.randomUUID(), expiresAt = Date.now() + 600000;
  const client = await db.pool.connect();
  try {
    await client.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
    const data = await snapshot(client, actor); await validate(client, actor, data, change);
    const affected = change.operation === "role" ? data.assignments.filter(a => a.roleId === change.id).map(a => data.subjects.find(s => s.userId === a.userId)?.name ?? a.userId) : [];
    const summary = change.operation === "grant" || change.operation === "invite" ? `${findSubject(data, change.subjectId).name} → ${data.roles.find(r => r.id === change.roleId)?.name} · ${change.scopeKind} · ${new Date(change.validUntil).toISOString()}` : change.operation === "revoke" ? "Withdraw the selected access assignment" : `${change.operation}: ${change.name}`;
    await client.query("INSERT INTO iam_access_previews VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [token, actor.companyId, actor.userId, actor.assignmentId, actor.sessionId, change, hash(data), expiresAt]);
    await client.query("COMMIT"); return { token, change, summary, affected, expiresAt };
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}
export async function applyPeopleAccess(db: PostgresDatabase, actor: IamActor, token: string, key: string) {
  db = iamDb(db);
  authorize(actor, true); await requireFreshMfa(actor);
  const result = await iamCommand(db, actor, key, "people.access.apply", { token }, "iam.access.apply", async client => {
    const preview = (await client.query(`SELECT * FROM iam_access_previews WHERE token=$1 AND company_id=$2 AND actor_id=$3 AND assignment_id=$4 AND session_id=$5 AND expires_at>$6 FOR UPDATE`, [token, actor.companyId, actor.userId, actor.assignmentId, actor.sessionId, Date.now()])).rows[0];
    if (!preview) throw new ApiError(409, "PREVIEW_EXPIRED", "Preview this change again.");
    const data = await snapshot(client, actor), change = peopleAccessChange.parse(json(preview.change));
    if (preview.fingerprint !== hash(data)) throw new ApiError(409, "PREVIEW_CHANGED", "People or permissions changed. Refresh and preview again.");
    await validate(client, actor, data, change);
    const id = "id" in change && change.id ? change.id : `${change.operation === "role" ? "custom_" : "iam-" + change.operation + "-"}${crypto.randomUUID()}`;
    if (change.operation === "organization") {
      await client.query(`INSERT INTO iam_external_organizations(id,company_id,name,kind) VALUES($1,$2,$3,$4) ON CONFLICT(id) DO UPDATE SET name=$3,kind=$4,version=iam_external_organizations.version+1`, [id, actor.companyId, change.name, change.kind]);
    } else if (change.operation === "team") {
      await client.query(`INSERT INTO iam_external_teams(id,company_id,organization_id,name,department_id) VALUES($1,$2,$3,$4,$5) ON CONFLICT(id) DO UPDATE SET organization_id=$3,name=$4,department_id=$5,version=iam_external_teams.version+1`, [id, actor.companyId, change.organizationId, change.name, change.departmentId]);
    } else if (change.operation === "subject") {
      const old = data.subjects.find(s => s.id === change.id);
      await client.query(`INSERT INTO iam_access_subjects(id,company_id,kind,name,email,organization_id,team_id,sponsor_person_id,expires_at,status,user_id,person_id) VALUES($1,$2,'external',$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO UPDATE SET name=$3,email=$4,organization_id=$5,team_id=$6,sponsor_person_id=$7,expires_at=$8,status=$9,version=iam_access_subjects.version+1`, [id, actor.companyId, change.name, change.email, change.organizationId, change.teamId, change.sponsorPersonId, change.expiresAt, change.status, old?.userId ?? null, old?.personId ?? null]);
    } else if (change.operation === "role") {
      await client.query(`INSERT INTO iam_roles(company_id,id,name,built_in) VALUES($1,$2,$3,false) ON CONFLICT(company_id,id) DO UPDATE SET name=$3`, [actor.companyId, id, change.name]);
      const revision = Number((await client.query("SELECT COALESCE(max(revision),0)+1 next FROM iam_role_revisions WHERE company_id=$1 AND role_id=$2", [actor.companyId, id])).rows[0].next);
      const retainedCeiling = data.roles.find(r=>r.id===change.id)?.grantablePermissions ?? [];
      await client.query(`INSERT INTO iam_role_revisions VALUES($1,$2,$3,$4,$5,$6,$7,$8)`, [actor.companyId, id, revision, JSON.stringify(change.permissions), JSON.stringify(retainedCeiling), JSON.stringify(change.scopes), actor.userId, Date.now()]);
      await client.query("UPDATE iam_roles SET published_revision=$3 WHERE company_id=$1 AND id=$2", [actor.companyId, id, revision]);
    } else if (change.operation === "revoke") {
      await client.query("UPDATE assignments SET revoked_at=$3,version=version+1 WHERE id=$1 AND company_id=$2", [change.assignmentId, actor.companyId, Date.now()]);
      return { id:change.assignmentId, change };
    } else {
      const subject = findSubject(data, change.subjectId), role = data.roles.find(r => r.id === change.roleId)!;
      const invitationId = change.operation === "invite" ? `iam-invite-${hash([actor.companyId, actor.userId, key]).slice(0, 40)}` : null;
      let existingUserId: string | null = null;
      if (invitationId) {
        const env = await runtimeEnv();
        const account = await env.CONTROL_DB.prepare('SELECT id FROM "user" WHERE lower(email)=lower(?)').bind(subject.email).first<{id:string}>();
        if (account) {
          if (!await env.CONTROL_DB.prepare("SELECT 1 FROM account WHERE userId=? AND providerId='credential'").bind(account.id).first()) throw new ApiError(409,"DIRECTORY_ACCOUNT_LINK_REQUIRED","Link this account through its verified directory identity.");
          if (data.subjects.some(s=>s.userId===account.id)) throw new ApiError(409,"ACCOUNT_ALREADY_LINKED","This account is already in this Company. Manage its existing user record.");
          existingUserId = account.id;
        }
      }
      const userId = subject.userId ?? existingUserId ?? `usr_invite_${invitationId}`;
      if (invitationId) {
        if (subject.kind === "internal") {
          await client.query(`INSERT INTO iam_access_subjects(id,company_id,kind,name,email,user_id,person_id,status) VALUES($1,$2,'internal',$3,$4,$5,$6,'invited') ON CONFLICT(id) DO UPDATE SET user_id=$5,status='invited',version=iam_access_subjects.version+1`, [subject.id, actor.companyId, subject.name, subject.email, userId, subject.personId]);
        } else await client.query("UPDATE iam_access_subjects SET user_id=$3,status='invited',version=version+1 WHERE id=$1 AND company_id=$2", [subject.id, actor.companyId, userId]);
        await client.query(`INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES($1,$2,$3,$4)`, [actor.companyId, userId, `principal-${invitationId}`, subject.personId]);
      }
      const grant = grantInput.parse({ ...change, userId, expectedRoleRevision: role.revision });
      await validateIamGrant(client, actor, grant);
      const assignmentId = await insertIamGrant(client, actor, grant, actor.assignmentId, "manual", null);
      if (invitationId) {
        await client.query(`INSERT INTO iam_access_invitations(id,company_id,subject_id,definition,author_id,parent_id,role_revision) VALUES($1,$2,$3,$4,$5,$6,$7)`, [invitationId, actor.companyId, subject.id, { ...grant, assignmentId, existingUserId, email: subject.email, name: subject.name }, actor.userId, actor.assignmentId, role.revision]);
        return { id: subject.id, invitationId, assignmentId, change };
      }
      return { id: subject.id, assignmentId, change };
    }
    return { id, change };
  });
  if ("invitationId" in result && result.invitationId) await prepareDirectInvitationDelivery(db, actor, result.invitationId);
  return result;
}
async function prepareDirectInvitationDelivery(db: PostgresDatabase, actor: IamActor, invitationId: string) {
  const row = await db.prepare("SELECT * FROM iam_access_invitations WHERE id=? AND company_id=? AND state='authorized'").bind(invitationId, actor.companyId).first<{ id: string; subject_id: string; parent_id: string; definition: { email: string; name: string; existingUserId: string | null; roleId: string; validUntil: number; scopeKind: string; scopeNodeId: string | null } }>();
  if (!row) throw unavailable();
  const env = await runtimeEnv(), token = createHmac("sha256", String(env.BETTER_AUTH_SECRET)).update(`invitation:${row.id}`).digest("hex"), tokenHash = createHash("sha256").update(token).digest("hex"), now = Date.now();
  const units = await accessUnitFacts(db, actor.companyId), local = units.find(u => u.id === row.definition.scopeNodeId);
  await env.CONTROL_DB.batch([
    env.CONTROL_DB.prepare(`INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,name,role,scope_kind,branch_id,department_id,created_by,assignment_id,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING`).bind(row.id, tokenHash, actor.holdingId, actor.companyId, row.subject_id, row.definition.email, row.definition.name, JSON.stringify({ role: row.definition.roleId, expires: row.definition.validUntil, direct: true, existingUserId: row.definition.existingUserId }), row.definition.scopeKind, local?.kind === "branch" ? local.id : local?.branchId ?? null, local?.kind === "department" ? local.id : null, actor.userId, row.parent_id, Math.min(now + 48 * 3600000, row.definition.validUntil), now),
    env.CONTROL_DB.prepare(`INSERT INTO account_invitation_delivery_jobs(id,invitation_id,holding_id,company_id,state,next_attempt_at,created_at,updated_at) VALUES(?,?,?,?,'queued',?,?,?) ON CONFLICT(invitation_id) DO NOTHING`).bind(`account-invite-delivery-${row.id}`, row.id, actor.holdingId, actor.companyId, now, now, now),
  ]);
}

export async function directInvitationEligible(db: SqlDatabase, row: Invitation, userId: string, now: number) {
  const invitation = await db.prepare(`SELECT i.*,s.kind,s.person_id,s.status,s.expires_at,s.sponsor_person_id,a.role AS author_role
    FROM iam_access_invitations i JOIN iam_access_subjects s ON s.company_id=i.company_id AND s.id=i.subject_id
    JOIN assignments a ON a.id=i.parent_id AND a.company_id=i.company_id
    JOIN assignments target ON target.id=i.definition->>'assignmentId' AND target.company_id=i.company_id AND target.user_id=?
    JOIN iam_roles r ON r.id=i.definition->>'roleId' AND r.company_id IN ('*',i.company_id)
    JOIN iam_role_revisions target_policy ON target_policy.company_id=r.company_id AND target_policy.role_id=r.id AND target_policy.revision=r.published_revision
    JOIN iam_roles author_role ON author_role.id=a.role AND author_role.company_id IN ('*',i.company_id)
    JOIN iam_role_revisions author_policy ON author_policy.company_id=author_role.company_id AND author_policy.role_id=author_role.id AND author_policy.revision=author_role.published_revision
    WHERE i.id=? AND i.company_id=? AND i.state IN ('authorized','accepted') AND s.user_id=? AND s.status!='suspended'
      AND (s.expires_at IS NULL OR s.expires_at>?) AND a.user_id=i.author_id
      AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'iam.access.apply')
      AND NOT iam_same_person(target.user_id,a.user_id,i.company_id)
      AND (target_policy.permissions_json||target_policy.grantable_json)<@author_policy.grantable_json
      AND target.revoked_at IS NULL AND target.valid_until>? AND r.published_revision=i.role_revision`).bind(userId, row.id, row.company_id, userId, now, now, now)
    .first<{kind:string;person_id:string|null;sponsor_person_id:string|null;author_role:string;state:string}>();
  if (!invitation) return null;
  const internal = await internalAccessPeople(db, row.company_id);
  if (invitation.kind === "internal" && !internal.some(p => p.personId === invitation.person_id && p.status === "active" && (!p.userId || p.userId === userId))) return null;
  if (invitation.kind === "external" && !internal.some(p => p.personId === invitation.sponsor_person_id && p.status === "active")) return null;
  return { role: invitation.author_role };
}
export async function activateDirectInvitation(db: PostgresDatabase, row: Invitation, userId: string, now: number) {
  const client = await db.pool.connect();
  try {
    await client.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
    const tx = transactionDatabase(client);
    if (!await directInvitationEligible(tx, row, userId, now)) throw new ApiError(409, "INVITATION_NO_LONGER_ELIGIBLE", "The relationship or authorizing access is no longer active.");
    const record = (await client.query("SELECT i.state,s.kind,s.person_id FROM iam_access_invitations i JOIN iam_access_subjects s ON s.id=i.subject_id AND s.company_id=i.company_id WHERE i.id=$1 AND i.company_id=$2 FOR UPDATE OF i,s", [row.id, row.company_id])).rows[0];
    if (record.state !== "accepted") {
      if (record.kind === "internal") await tx.batch(stageInvitationPersonLinks(tx, userId, record.person_id, now));
      await client.query("UPDATE iam_access_subjects SET status='active',version=version+1 WHERE id=$1 AND company_id=$2", [row.person_id, row.company_id]);
      await client.query("UPDATE iam_access_invitations SET state='accepted' WHERE id=$1 AND company_id=$2", [row.id, row.company_id]);
      await client.query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,'invitation.accept',$5,$6,$7)", [crypto.randomUUID(), row.company_id, userId, row.assignment_id, row.id, { subjectId: row.person_id, method: "direct-invitation" }, now]);
    }
    await client.query("COMMIT");
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}
