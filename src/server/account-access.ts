import { requireBusinessRole } from "./responsibilities";
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
    scope: z.enum(["holding", "branch"]),
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
function owner(actor: WorkContext) { return requireBusinessRole(actor,["holding_owner"]); }
export async function grantPersonAccess(
  actor: WorkContext,
  personId: string,
  input: z.infer<typeof grantSchema>,
  key: string,
) {
  actor = owner(actor);
  const person = await directoryPerson(actor, personId);
  const { write } = await holdingResources(actor.holdingId);
  if (!person.user_id)
    throw new ApiError(
      409,
      "INVITE_REQUIRED",
      "Invite this person to activate an account first.",
    );
  if (person.user_id === actor.userId)
    throw new ApiError(
      403,
      "SELF_ACCESS_CHANGE",
      "Another holding owner must change your access.",
    );
  if (person.status !== "active" || person.engagement_status !== "active")
    throw new ApiError(
      409,
      "PERSON_INACTIVE",
      "Activate the employee before granting access.",
    );
  const expires = Date.parse(input.expiresAt) + 86400000;
  if (expires <= Date.now())
    throw new ApiError(422, "EXPIRY_INVALID", "Choose a future access expiry.");
  if (input.scope === "branch" && !person.branch_id)
    throw new ApiError(
      422,
      "BRANCH_REQUIRED",
      "Assign an employee branch first.",
    );
  const id = `assignment-${key}`;
  return executeCommand(
    write,
    actor,
    key,
    "identity.assignment.grant",
    input,
    personId,
    Number(person.version),
    { id, personId },
    [
      assertion(
        write,
        key,
        "person",
        "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND user_id=? AND status='active')",
        [personId, person.user_id],
      ),
      write
        .prepare(
          "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id) VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          person.user_id,
          input.role,
          JSON.stringify(businessRoles[input.role]),
          actor.companyId,
          input.scope === "branch" ? person.branch_id : null,
          Date.now(),
          expires,
          input.scope,
          input.scope === "branch" ? person.branch_id : null,
        ),
    ],
  );
}
export async function revokePersonAccess(
  actor: WorkContext,
  personId: string,
  assignmentId: string,
  key: string,
) {
  actor = owner(actor);
  const person = await directoryPerson(actor, personId);
  if (person.user_id === actor.userId)
    throw new ApiError(
      403,
      "SELF_ACCESS_CHANGE",
      "Another holding owner must change your access.",
    );
  const { write } = await holdingResources(actor.holdingId);
  return executeCommand(
    write,
    actor,
    key,
    "identity.assignment.revoke",
    { assignmentId },
    personId,
    Number(person.version),
    { id: assignmentId, personId },
    [
      assertion(
        write,
        key,
        "assignment-owner",
        "EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role!='holding_owner')",
        [assignmentId, person.user_id, actor.companyId],
      ),
      write
        .prepare(
          "UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?",
        )
        .bind(Date.now(), assignmentId),
    ],
  );
}
export async function invitePerson(
  actor: WorkContext,
  personId: string,
  input: z.infer<typeof grantSchema>,
) {
  actor = owner(actor);
  const person = await directoryPerson(actor, personId);
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
  const id = crypto.randomUUID(),
    token = Array.from(crypto.getRandomValues(new Uint8Array(32)), (v) =>
      v.toString(16).padStart(2, "0"),
    ).join("");
  await env.CONTROL_DB.batch([
    env.CONTROL_DB.prepare(
      "UPDATE account_invitations SET status='revoked' WHERE holding_id=? AND person_id=? AND status='pending'",
    ).bind(actor.holdingId, personId),
    env.CONTROL_DB.prepare(
      "INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,name,role,scope_kind,branch_id,created_by,assignment_id,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
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
      input.scope === "branch" ? person.branch_id : null,
      actor.userId,
      actor.assignmentId,
      Date.now() + 48 * 3600000,
      Date.now(),
    ),
  ]);
  return { id, token, expiresAt: Date.now() + 48 * 3600000 };
}
type Invitation = {
  id: string;
  holding_id: string;
  company_id: string;
  person_id: string;
  email: string;
  name: string;
  role: string;
  scope_kind: "holding" | "branch";
  branch_id: string | null;
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
  const userId = `usr_invite_${row.id}`,
    now = Date.now();
  const grant = JSON.parse(row.role) as { role: BusinessRole; expires: number };
  const eligible = await write
    .prepare(
      "SELECT 1 FROM hr_people p JOIN assignments a ON a.id=? WHERE p.id=? AND p.company_id=? AND p.status='active' AND EXISTS(SELECT 1 FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.status='active') AND (p.user_id IS NULL OR p.user_id=?) AND a.user_id=? AND a.role='holding_owner' AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?",
    )
    .bind(
      row.assignment_id,
      row.person_id,
      row.company_id,
      userId,
      row.created_by,
      now,
      now,
    )
    .first();
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
    role: "holding_owner",
    companyId: row.company_id,
    branchId: null,
    departmentId: null,
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
    [
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
          "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id) VALUES(?,?,?,?,?,?,?,?,?,?)",
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
          row.branch_id,
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
