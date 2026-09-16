import {createHmac} from "node:crypto";
import { iamCommand } from "./iam-transactions";
import { requireFreshMfa } from "./iam-mfa";
import type { PostgresDatabase } from "./runtime/postgres";
import { iamActorFromWork } from "./iam-context";
import { grantIamAccess,grantInput,revokeIamAssignment,validateIamGrant,proposeIamReview } from "./iam-service";
import { requirePermission } from "@/server/iam-permissions";
import { organizationGuard } from "./organization";
import { z } from "zod";
import { hashPassword, verifyPassword } from "better-auth/crypto";
import type { WorkContext } from "./authorization";
import { runtimeEnv, holdingResources } from "./runtime";
import { directoryPerson } from "./people-directory";
import { ApiError } from "./http";
import { assertion, executeCommand } from "./commands";
import { businessRoles, type BusinessRole } from "@/lib/business-roles";
export const grantSchema = z
  .object({
    role: z.enum(
      Object.keys(businessRoles) as [BusinessRole, ...BusinessRole[]],
    ),
    scope: z.enum(["holding", "branch", "department"]),
    expiresAt: z.iso.date(),
  })
  .refine((input) => input.role !== "it_admin" || input.scope === "holding", {
    message: "IT administrators require company scope.",
    path: ["scope"],
  });
export const acceptInvitationSchema = z.object({
  token: z.string().min(32).max(128),
  password: z.string().min(12).max(128),
});
async function hash(value: string) {
  const b = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(value),
  );
  return Array.from(new Uint8Array(b), (v) =>
    v.toString(16).padStart(2, "0"),
  ).join("");
}
function owner(actor: WorkContext) { return requirePermission(actor,"iam.manage"); }
function grantScope(actor:WorkContext,input:z.infer<typeof grantSchema>,person:Record<string,string|number|null>) {
  if ((actor.assignmentScope === "branch" && input.scope === "holding") || (actor.assignmentScope === "department" && input.scope !== "department")) throw new ApiError(403,"GRANT_SCOPE_FORBIDDEN","The role must stay within your HR assignment scope.");
  if ((input.scope === "branch" && !person.branch_id) || (input.scope === "department" && !person.department_id)) throw new ApiError(422,"PLACEMENT_REQUIRED","Set the staff placement before granting this scope.");
}
export async function grantPersonAccess(actor:WorkContext,personId:string,input:z.infer<typeof grantSchema>,key:string){
 const person=await directoryPerson(actor,personId);if(person.user_id===actor.userId)throw new ApiError(403,"SELF_ACCESS_CHANGE","Another administrator must change your access.");grantScope(actor,input,person);
 if(!person.user_id)throw new ApiError(409,"INVITE_REQUIRED","Invite and activate this account first.");
 if(person.status!=="active"||person.engagement_status!=="active")throw new ApiError(409,"PERSON_INACTIVE","Activate the employee before granting access.");
 const iam={db:(await holdingResources(actor.holdingId)).write as PostgresDatabase,actor:iamActorFromWork(actor)};
 return {...await grantIamAccess(iam.db,iam.actor,grantInput.parse({userId:person.user_id,roleId:input.role,scopeKind:input.scope,scopeNodeId:input.scope==="holding"?null:input.scope==="branch"?person.branch_id:person.department_id,validUntil:Date.parse(input.expiresAt)+86400000,reason:"Reviewed access requested from employee profile"}),key),personId};
}
export async function revokePersonAccess(actor:WorkContext,personId:string,assignmentId:string,key:string){
 const person=await directoryPerson(actor,personId),iam={db:(await holdingResources(actor.holdingId)).write as PostgresDatabase,actor:iamActorFromWork(actor)};
 const target=await iam.db.prepare("SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=?").bind(assignmentId,person.user_id,actor.companyId).first<{version:number}>();
 if(!target)throw new ApiError(404,"ASSIGNMENT_NOT_FOUND","Assignment not found for this employee.");
 return {...await revokeIamAssignment(iam.db,iam.actor,assignmentId,target.version,"Revoked from employee profile",key),personId};
}
export async function invitePerson(
  actor: WorkContext,
  personId: string,
  input: z.infer<typeof grantSchema>,
  key:string,
) {
  actor = owner(actor);
  const person = await directoryPerson(actor, personId);
  grantScope(actor,input,person);
  if (person.user_id)
    throw new ApiError(
      409,
      "ACCOUNT_ALREADY_LINKED",
      "This employee already has a login account.",
    );
  if (
    !person.email ||
    person.status !== "active" ||
    person.engagement_status !== "active"
  )
    throw new ApiError(
      422,
      "PERSON_NOT_READY",
      "An active employee with a work email is required.",
    );
  const env = await runtimeEnv();
  if (
    await env.CONTROL_DB.prepare(
      'SELECT 1 FROM "user" WHERE lower(email)=lower(?)',
    )
      .bind(person.email)
      .first()
  )
    throw new ApiError(
      409,
      "ACCOUNT_EXISTS",
      "That email already has an account. Existing-account linking requires identity verification.",
    );
  const expiry = Date.parse(input.expiresAt) + 86400000;
  if (expiry <= Date.now())
    throw new ApiError(422, "EXPIRY_INVALID", "Choose a future access expiry.");
  const iam=iamActorFromWork(actor),{write}=await holdingResources(actor.holdingId);await requireFreshMfa(iam);
  const id=`invite-${(await hash(actor.companyId+':'+actor.userId+':'+key)).slice(0,40)}`;
  await iamCommand(write as PostgresDatabase,iam,key,'invitation.reserve',{personId,...input},'iam.manage',async()=>({id}));
  const token=createHmac('sha256',env.BETTER_AUTH_SECRET).update('invitation:'+id).digest('hex');
  await env.CONTROL_DB.batch([
    env.CONTROL_DB.prepare(
      "UPDATE account_invitations SET status='revoked' WHERE holding_id=? AND person_id=? AND status='pending' AND id!=?",
    ).bind(actor.holdingId, personId,id),
    env.CONTROL_DB.prepare(
      "INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,name,role,scope_kind,branch_id,created_by,assignment_id,expires_at,created_at,department_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING",
    ).bind(
      id,
      await hash(token),
      actor.holdingId,
      actor.companyId,
      personId,
      person.email,
      person.legal_name,
      JSON.stringify({ role: input.role, expires: expiry }),
      input.scope,
      input.scope !== "holding" ? person.branch_id : null,
      actor.userId,
      actor.assignmentId,
      Date.now() + 48 * 3600000,
      Date.now(),
      input.scope === "department" ? person.department_id : null,
    ),
  ]);
  const requested=grantInput.parse({userId:`usr_invite_${id}`,roleId:input.role,scopeKind:input.scope,scopeNodeId:input.scope==="holding"?null:input.scope==="branch"?person.branch_id:person.department_id,validUntil:expiry,reason:"Invite employee with explicitly reviewed access"});
  const review=await iamCommand(write as PostgresDatabase,iam,`invite-review-${id}`,"invitation.propose",{invitationId:id,...requested},"iam.manage",async c=>{await validateIamGrant(c,iam,requested);return proposeIamReview(c,iam,"invitation.authorize",{invitationId:id,...requested,parentId:iam.assignmentId},"Authorize an employee invitation and initial grant.");});
  return { id, token, reviewId:review.id, message:"Invitation prepared. Independent approval is required before activation.",expiresAt: Date.now() + 48 * 3600000 };
}
type Invitation = {
  id: string;
  holding_id: string;
  company_id: string;
  person_id: string;
  email: string;
  name: string;
  role: string;
  scope_kind: "holding" | "branch" | "department";
  branch_id: string | null;
  department_id: string | null;
  created_by: string;
  assignment_id: string;
  expires_at: number;
  status: string;
};
async function invitation(token: string) {
  const env = await runtimeEnv();
  const row = await env.CONTROL_DB.prepare(
    "SELECT * FROM account_invitations WHERE token_hash=? AND (status='pending' OR (status='accepted' AND accepted_at IS NULL)) AND expires_at>?",
  )
    .bind(await hash(token), Date.now())
    .first<Invitation>();
  if (!row)
    throw new ApiError(
      410,
      "INVITATION_UNAVAILABLE",
      "This invitation is expired, revoked or already accepted.",
    );
  return { env, row };
}
export async function inspectInvitation(token: string) {
  const { row } = await invitation(token);
  return { name: row.name, email: row.email };
}
export async function acceptInvitation(
  input: z.infer<typeof acceptInvitationSchema>,
) {
  const { env, row } = await invitation(input.token);
  const { write } = await holdingResources(row.holding_id);
  const reviewed=await write.prepare("SELECT id FROM iam_reviews WHERE company_id=? AND kind='invitation.authorize' AND status='approved' AND payload->>'invitationId'=?").bind(row.company_id,row.id).first();
  if(!reviewed)throw new ApiError(409,"INVITATION_REVIEW_REQUIRED","An independent administrator must approve this invitation before activation.");
  const guard=await organizationGuard(write,row.company_id,`activate-${row.id}`);
  const userId = `usr_invite_${row.id}`,
    now = Date.now();
  const grant = JSON.parse(row.role) as { role: BusinessRole; expires: number };
  const eligible = await write
    .prepare(
      "SELECT a.role FROM hr_people p JOIN assignments a ON a.id=? AND a.company_id=p.company_id WHERE p.id=? AND p.company_id=? AND p.status='active' AND EXISTS(SELECT 1 FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)) AND (p.user_id IS NULL OR p.user_id=?) AND a.user_id=? AND iam_role_allows(a.role,a.company_id,'iam.manage') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND ? IN ('branch','department') AND a.scope_node_id=?) OR (a.scope_kind='department' AND ?='department' AND a.scope_node_id=?)) AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?",
    )
    .bind(
      row.assignment_id,
      row.person_id,
      row.company_id,
      row.branch_id,row.branch_id,row.department_id,row.department_id,
      userId,
      row.created_by,
      row.scope_kind,row.branch_id,row.scope_kind,row.department_id,
      now,
      now,
    )
    .first<{role:string}>();
  if (!eligible || grant.expires <= now)
    throw new ApiError(
      409,
      "INVITATION_NO_LONGER_ELIGIBLE",
      "The employee or authorizing assignment is no longer active.",
    );
  const account = await env.CONTROL_DB.prepare(
    "SELECT password FROM account WHERE userId=? AND providerId='credential'",
  )
    .bind(userId)
    .first<{ password: string }>();
  if (
    account &&
    !(await verifyPassword({
      password: input.password,
      hash: account.password,
    }))
  )
    throw new ApiError(
      409,
      "ACTIVATION_RETRY_PASSWORD",
      "Use the password from your first activation attempt.",
    );
  if (!account)
    await env.CONTROL_DB.batch([
      env.CONTROL_DB.prepare(
        "INSERT INTO account_invitation_claims(invitation_id,eligible,claimed_at) VALUES(?,CASE WHEN EXISTS(SELECT 1 FROM account_invitations WHERE id=? AND status='pending' AND expires_at>?) THEN 1 ELSE 0 END,?)",
      ).bind(row.id, row.id, now, now),
      env.CONTROL_DB.prepare(
        'INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,0,?,?)',
      ).bind(userId, row.name, row.email, now, now),
      env.CONTROL_DB.prepare(
        "INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)",
      ).bind(
        `account-${row.id}`,
        userId,
        userId,
        await hashPassword(input.password),
        now,
        now,
      ),
      env.CONTROL_DB.prepare(
        "INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)",
      ).bind(`principal-${row.id}`, userId, row.name, now),
      env.CONTROL_DB.prepare(
        "UPDATE account_invitations SET status='accepted',accepted_user_id=? WHERE id=? AND status='pending'",
      ).bind(userId, row.id),
    ]);
  // A failure between databases leaves an account without access. Reusing the invitation
  // completes the same stable assignment; it never grants an unverified different user.
  const actor = {
    userId: row.created_by,
    assignmentId: row.assignment_id,
    role: eligible.role,
    companyId: row.company_id,
    branchId: row.branch_id,
    departmentId: row.department_id,
  };
  await executeCommand(
    write,
    actor,
    `activate-${row.id}`,
    "identity.account.activate",
    { invitationId: row.id },
    row.person_id,
    1,
    { userId },
    [guard,
      assertion(write,`activate-${row.id}`,"reviewed-invitation","EXISTS(SELECT 1 FROM iam_reviews review JOIN iam_roles r ON r.id=review.payload->>'roleId' AND r.company_id IN ('*',review.company_id) WHERE review.id=? AND review.company_id=? AND review.status='approved' AND r.published_revision=(review.payload->>'expectedRoleRevision')::integer)",[reviewed.id as string,row.company_id]),
      assertion(
        write,
        `activate-${row.id}`,
        "person",
        "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND status='active' AND (user_id IS NULL OR user_id=?))",
        [row.person_id, row.company_id, userId],
      ),
      write
        .prepare(
          "UPDATE hr_people SET user_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(userId, now, row.person_id),
      write
        .prepare("UPDATE employees SET user_id=? WHERE person_id=?")
        .bind(userId, row.person_id),
      write
        .prepare(
          "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id,parent_id,source) VALUES(?,?,?,?,?,?,?,?,?,?,?,'manual')",
        )
        .bind(
          `assignment-${row.id}`,
          userId,
          grant.role,
          JSON.stringify(businessRoles[grant.role]),
          row.company_id,
          row.branch_id,
          now,
          grant.expires,
          row.scope_kind,
          row.scope_kind === "department" ? row.department_id : row.branch_id,
          row.assignment_id,
        ),
    ],
  );
  await env.CONTROL_DB.prepare(
    "UPDATE account_invitations SET status='accepted',accepted_user_id=?,accepted_at=? WHERE id=? AND status='accepted'",
  )
    .bind(userId, Date.now(), row.id)
    .run();
  return { activated: true };
}
