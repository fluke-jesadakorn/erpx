import type { SqlDatabase } from "./runtime/types";
import { z } from "zod";
import { assertion, executeCommand } from "./commands";
import { ApiError } from "./http";
import {
  authorityGuard,
  requireCompanyAuthority,
  type StaffActor,
} from "./staff-policy";
import {
  branchStatements,
  memberStatements,
  requireVerified,
  type BetaUser,
} from "./beta-onboarding";
import { sha256 } from "./crypto";
export const invitationSchema = z.object({
  email: z.email().transform((v) => v.toLowerCase()),
  branchId: z.string().min(1).max(160),
  role: z.enum(["employee", "it_admin"]).default("employee"),
});
export async function inviteMember(
  db: SqlDatabase,
  actor: StaffActor,
  rawInput: z.input<typeof invitationSchema>,
  key: string,
  token: string,
) {
  const input = invitationSchema.parse(rawInput);
  await requireCompanyAuthority(db, actor, ["holding_owner"]);
  const now = Date.now(),
    id = `invite-${key}`;
  return executeCommand(db, actor, key, "beta.invite", input, id, 1, { id }, [
    authorityGuard(db, key, actor, ["holding_owner"]),
    db
      .prepare(
        "UPDATE beta_team_invitations SET revoked_at=? WHERE expires_at<=? AND accepted_at IS NULL AND revoked_at IS NULL",
      )
      .bind(now, now),
    assertion(
      db,
      key,
      "member-capacity",
      `(SELECT count(*) FROM beta_members m JOIN hr_people p ON p.user_id=m.user_id WHERE p.company_id=?)+(SELECT count(*) FROM beta_team_invitations i JOIN organization_nodes b ON b.id=i.branch_id WHERE b.company_id=? AND i.accepted_at IS NULL AND i.revoked_at IS NULL AND i.expires_at>?) < (SELECT member_limit FROM beta_policy WHERE company_id=?)`,
      [actor.companyId, actor.companyId, now, actor.companyId],
    ),
    assertion(
      db,
      key,
      "invite-branch",
      "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",
      [input.branchId, actor.companyId],
    ),
    assertion(
      db,
      key,
      "existing-member",
      "NOT EXISTS(SELECT 1 FROM hr_people WHERE lower(email)=? AND user_id IS NOT NULL)",
      [input.email],
    ),
    db
      .prepare(
        "INSERT INTO beta_team_invitations(id,email,branch_id,invited_by,token_hash,expires_at,created_at,role) VALUES(?,?,?,?,?,?,?,?)",
      )
      .bind(
        id,
        input.email,
        input.branchId,
        actor.userId,
        await sha256(token),
        now + 7 * 86400000,
        now,
        input.role,
      ),
  ]);
}
export async function acceptMember(
  db: SqlDatabase,
  user: BetaUser,
  token: string,
) {
  requireVerified(user);
  const row = await db
    .prepare("SELECT * FROM beta_team_invitations WHERE token_hash=?")
    .bind(await sha256(token))
    .first<{
      id: string;
      email: string;
      branch_id: string;
      invited_by: string;
      expires_at: number;
      accepted_user_id: string | null;
      revoked_at: number | null;
      role: "employee" | "it_admin";
    }>();
  if (
    !row ||
    row.email !== user.email.toLowerCase() ||
    row.revoked_at ||
    row.expires_at <= Date.now()
  )
    throw new ApiError(
      404,
      "INVITE_UNAVAILABLE",
      "Sign in with the verified email that was invited, or request a new invitation.",
    );
  if (row.accepted_user_id) {
    if (row.accepted_user_id !== user.id)
      throw new ApiError(409, "INVITE_USED", "Invitation already used.");
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM assignments WHERE id=? AND user_id=? AND revoked_at IS NULL AND valid_until>?",
        )
        .bind(`beta-assignment-${user.id}`, user.id, Date.now())
        .first())
    )
      throw new ApiError(
        403,
        "ACCESS_REVOKED",
        "This company access was revoked. Ask the owner for a new invitation.",
      );
    return { joined: true };
  }
  const branch = await db
    .prepare(
      "SELECT company_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'",
    )
    .bind(row.branch_id)
    .first<{ company_id: string }>();
  if (!branch)
    throw new ApiError(409, "BRANCH_UNAVAILABLE", "Branch unavailable.");
  const key = `accept-${row.id}`,
    now = Date.now();
  try {
    await db.batch([
      authorityGuard(
        db,
        key,
        { userId: row.invited_by, companyId: branch.company_id },
        ["holding_owner"],
      ),
      assertion(
        db,
        key,
        "invite",
        "EXISTS(SELECT 1 FROM beta_team_invitations WHERE id=? AND email=? AND accepted_user_id IS NULL AND revoked_at IS NULL AND expires_at>?)",
        [row.id, user.email.toLowerCase(), now],
      ),
      assertion(
        db,
        key,
        "capacity",
        "(SELECT count(*) FROM beta_members m JOIN hr_people p ON p.user_id=m.user_id WHERE p.company_id=?)<(SELECT member_limit FROM beta_policy WHERE company_id=?)",
        [branch.company_id, branch.company_id],
      ),
      ...memberStatements(
        db,
        branch.company_id,
        row.branch_id,
        user,
        false,
        row.id,
      ),
      ...(row.role === "it_admin"
        ? [
            db
              .prepare(
                "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'it_admin','[\"collaboration\"]',?,?,?,'holding')",
              )
              .bind(
                `beta-it-${row.id}`,
                user.id,
                branch.company_id,
                now,
                now + 10 * 365 * 86400000,
              ),
          ]
        : []),
      db
        .prepare(
          "UPDATE beta_team_invitations SET accepted_user_id=?,accepted_at=? WHERE id=?",
        )
        .bind(user.id, now, row.id),
      db
        .prepare(
          "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,'beta.member.join','beta_member',?,1,'Verified invitation acceptance',?)",
        )
        .bind(
          crypto.randomUUID(),
          key,
          user.id,
          `beta-assignment-${user.id}`,
          user.id,
          now,
        ),
      db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key),
    ]);
  } catch {
    if (
      (
        await db
          .prepare(
            "SELECT accepted_user_id FROM beta_team_invitations WHERE id=?",
          )
          .bind(row.id)
          .first<{ accepted_user_id: string }>()
      )?.accepted_user_id !== user.id
    )
      throw new ApiError(
        409,
        "INVITE_CHANGED",
        "The invitation or company access changed. Request a new invitation.",
      );
  }
  return { joined: true };
}
export async function addBetaBranch(
  db: SqlDatabase,
  actor: StaffActor,
  name: string,
  key: string,
) {
  await requireCompanyAuthority(db, actor, ["holding_owner"]);
  const id = `branch-${key}`;
  return executeCommand(
    db,
    actor,
    key,
    "beta.branch.create",
    { name },
    id,
    1,
    { id },
    [
      authorityGuard(db, key, actor, ["holding_owner"]),
      assertion(
        db,
        key,
        "branch-capacity",
        "(SELECT count(*) FROM organization_nodes WHERE kind='branch' AND company_id=?)<(SELECT branch_limit FROM beta_policy WHERE company_id=?)",
        [actor.companyId, actor.companyId],
      ),
      ...branchStatements(
        db,
        actor.companyId,
        id,
        z.string().trim().min(2).max(100).parse(name),
        id,
      ),
    ],
  );
}
