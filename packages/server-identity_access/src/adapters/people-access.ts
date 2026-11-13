import "server-only";
import { createHash } from "node:crypto";
import type { PoolClient } from "pg";
import type { PeopleAccessSnapshot, AccessSubject, PeopleRelationshipsSnapshot } from "@erpx/feature-identity_access/lib/people-access";
import { permissionCatalog, hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { transactionDatabase, type PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { internalAccessPeople } from "@erpx/server-people_payroll/public/iam-subject-facts";
import { accessUnitFacts } from "@erpx/server-organization/public/iam-unit-facts";
import { stageInvitationPersonLinks } from "@erpx/server-people_payroll/public/identity-person-link-effects";
import type { Invitation } from "@erpx/server-identity_access/domain/account-access";
import type { IamActor } from "@erpx/server-identity_access/domain/iam-actor";
import { assertCompanyScope } from "@erpx/server-identity_access/public/iam-context";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { iamDb } from "@erpx/server-identity_access/public/iam-db";

/**
 * People access reads plus the direct-invitation acceptance guard.
 *
 * Preview/apply writes for relationships, accounts, roles, grants, invitations
 * and withdrawal are owned by the native Rust Identity Access service
 * (`public/people_grants.ts` and `public/people_access.ts`); this module keeps
 * only the snapshot reader and the acceptance-side eligibility check that the
 * TypeScript Account acceptance flow still calls.
 */
const hash = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const json = <T,>(value: T | string): T => typeof value === "string" ? JSON.parse(value) : value;
const unavailable = () => new ApiError(404, "ACCESS_OBJECT_UNAVAILABLE", "This object is unavailable in your selected Company.");
function authorize(actor: IamActor, write = false, relationships = false) {
  if (actor.platform) throw new ApiError(403, "COMPANY_SCOPE_REQUIRED", "Select a Company workspace for People access.");
  assertCompanyScope(actor);
  requirePermission(actor, relationships ? write ? "people.relationships.manage" : "people.relationships.read" : write ? "iam.access.apply" : "iam.read");
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
    client.query(`SELECT user_id,person_id,login_policy FROM iam_members WHERE company_id=$1`, [company]),
    client.query(`SELECT id,status,user_id,name,email,person_id,version FROM iam_access_subjects WHERE company_id=$1 AND kind='internal'`, [company]),
  ]);
  const ceiling = actor.iam?.grantablePermissions ?? [];
  const availableRoles = roles.rows.map(r => ({ ...r, permissions:json<string[]>(r.permissions),grantable:json<string[]>(r.grantable),scopes:json<string[]>(r.scopes) }));
  const employees: AccessSubject[] = internal.map(p => ({ ...p, id: `employee:${p.personId}`, kind: "internal", organizationId: null, teamId: null, sponsorPersonId: null, expiresAt: null, version: 0, status: internalSubjects.rows.find(s => s.id === `employee:${p.personId}`)?.status ?? p.status }));
  const subjects: AccessSubject[] = [...employees, ...external.rows.map(p => ({ ...p, expiresAt: p.expiresAt == null ? null : Number(p.expiresAt) }))];
  for (const account of internalSubjects.rows.filter(s => !s.person_id && !subjects.some(p => p.userId && p.userId === s.user_id))) {
    subjects.push({ id: account.id, name: account.name, email: account.email, kind: "unclassified", userId: account.user_id, personId: null, departmentId: null, organizationId: null, teamId: null, sponsorPersonId: null, expiresAt: null, status: account.status, version: account.version });
  }
  for (const subject of subjects) {
    subject.classification = subject.kind === "internal" ? "employee" : subject.kind;
    subject.linkedUserIds = [...new Set([...(subject.userId ? [subject.userId] : []), ...internal.filter(p => subject.personId && p.personId === subject.personId && p.userId).map(p => p.userId!), ...members.rows.filter(m => subject.personId && m.person_id === subject.personId).map(m => m.user_id)])].sort();
    subject.accountSource = subject.userId ? members.rows.find(m => m.user_id === subject.userId)?.login_policy === "directory" ? "directory" : "local" : null;
  }
  const missingUsers = [...new Set([...assignments.rows.map(a => a.userId), ...members.rows.map(m => m.user_id)])].filter(userId => !subjects.some(s => s.linkedUserIds?.includes(userId)));
  if (missingUsers.length) {
    const env = await runtimeEnv();
    const accounts = (await env.CONTROL_DB.prepare(`SELECT id,name,email FROM "user" WHERE id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY name,id`).bind(JSON.stringify(missingUsers)).all<{id:string;name:string;email:string}>()).results;
    for (const account of accounts) {
      const personId = members.rows.find(m => m.user_id === account.id)?.person_id ?? null, employee = employees.find(p => p.personId === personId);
      subjects.push({ id: `account:${hash([company, account.id]).slice(0, 40)}`, name: account.name, email: account.email, userId: account.id, personId, kind: employee ? "internal" : "unclassified", classification: employee ? "employee" : "unclassified", linkedUserIds: [account.id], accountSource: members.rows.find(m => m.user_id === account.id)?.login_policy === "directory" ? "directory" : "local", departmentId: employee?.departmentId ?? null, organizationId: null, teamId: null, sponsorPersonId: null, expiresAt: null, status: "active", version: 0 });
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
function relationshipsOnly(data: PeopleAccessSnapshot, actor: IamActor): PeopleRelationshipsSnapshot {
  return { subjects: data.subjects, organizations: data.organizations, teams: data.teams, units: data.units, canManage: hasPermission(actor, "people.relationships.manage") };
}
export async function peopleRelationshipsSnapshot(db: PostgresDatabase, actor: IamActor) {
  authorize(actor, false, true);
  return relationshipsOnly(await readSnapshot(db, actor), actor);
}
export async function peopleAccessSnapshot(db: PostgresDatabase, actor: IamActor) {
  authorize(actor);
  return readSnapshot(db, actor);
}
async function readSnapshot(db: PostgresDatabase, actor: IamActor) {
  db = iamDb(db);
  const client = await db.pool.connect();
  try { await client.query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY"); const result = await snapshot(client, actor); await client.query("COMMIT"); return result; }
  catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}

/**
 * Directory-invitation acceptance guard retained until the Account acceptance
 * flow moves to the native route. removalTaskId:
 * `retire.identity_access.account-invitation-acceptance`; removalCondition: the
 * TS `createAccountAccessService` acceptance path is replaced by the native
 * Account invitation route, which then owns direct eligibility and activation.
 */
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
  if (invitation.kind === "internal" && invitation.person_id && !internal.some(p => p.personId === invitation.person_id && p.status === "active" && (!p.userId || p.userId === userId))) return null;
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
      if (record.kind === "internal" && record.person_id) await tx.batch(stageInvitationPersonLinks(tx, userId, record.person_id, now));
      await client.query("UPDATE iam_access_subjects SET status='active',version=version+1 WHERE id=$1 AND company_id=$2", [row.person_id, row.company_id]);
      await client.query("UPDATE iam_access_invitations SET state='accepted' WHERE id=$1 AND company_id=$2", [row.id, row.company_id]);
      await client.query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,'invitation.accept',$5,$6,$7)", [crypto.randomUUID(), row.company_id, userId, row.assignment_id, row.id, { subjectId: row.person_id, method: "direct-invitation" }, now]);
    }
    await client.query("COMMIT");
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}
