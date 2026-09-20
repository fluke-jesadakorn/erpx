import "server-only";
import { enrollmentToken } from "./enrollment";
import type { SqlDatabase } from "../runtime/types";
import type { BetaEnvironment } from "../beta-config";
import { sendBetaEmail } from "../beta-email";
export async function drainLegalInvitations(
  db: SqlDatabase,
  env: BetaEnvironment & {
    BETTER_AUTH_URL?: string;
    BETTER_AUTH_SECRET?: string;
  },
  send = sendBetaEmail,
) {
  const now = Date.now();
  await db
    .prepare(
      "UPDATE legal_invitation_jobs SET state=CASE WHEN attempts<6 THEN 'queued' ELSE 'failed' END,lease_token=NULL,lease_until=NULL,error='DELIVERY_INTERRUPTED' WHERE state='running' AND lease_until<?",
    )
    .bind(now)
    .run();
  const token = crypto.randomUUID(),
    j = await db
      .prepare(
        "UPDATE legal_invitation_jobs SET state='running',attempts=attempts+1,lease_token=?,lease_until=? WHERE id=(SELECT id FROM legal_invitation_jobs WHERE state='queued' AND next_attempt_at<=? ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *",
      )
      .bind(token, now + 30000, now)
      .first<{
        id: string;
        share_id: string;
        holding_slug: string;
        attempts: number;
      }>();
  if (!j) return;
  try {
    const share = await db
      .prepare(
        "SELECT s.email,s.expires_at FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_settings c ON c.company_id=i.company_id AND c.enabled WHERE s.id=? AND s.revoked_at IS NULL AND s.expires_at>?",
      )
      .bind(j.share_id, Date.now())
      .first<{ email: string; expires_at: number }>();
    if (!share) {
      await db
        .prepare(
          "UPDATE legal_invitation_jobs SET state='cancelled',lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
        )
        .bind(j.id, token)
        .run();
      return;
    }
    if (!env.BETTER_AUTH_URL) throw new Error("PUBLIC_ORIGIN_NOT_CONFIGURED");
    const url = new URL(
      `/legal-portal/${encodeURIComponent(j.holding_slug)}`,
      env.BETTER_AUTH_URL,
    ).href;
    const inviteLink = env.BETTER_AUTH_SECRET
      ? new URL(
          `/legal-portal/enroll#token=${await enrollmentToken(env.BETTER_AUTH_SECRET, j.holding_slug, j.share_id, share.expires_at)}`,
          env.BETTER_AUTH_URL,
        ).href
      : null;
    await send(
      env,
      share.email,
      "You have an ERPX Legal document invitation",
      inviteLink ? `${url}\n\nNew invited account: ${inviteLink}` : url,
      `legal-invitation-${j.id}`,
    );
    await db
      .prepare(
        "UPDATE legal_invitation_jobs SET state='sent',sent_at=?,lease_token=NULL,lease_until=NULL,error=NULL WHERE id=? AND lease_token=?",
      )
      .bind(Date.now(), j.id, token)
      .run();
  } catch (e) {
    await db
      .prepare(
        "UPDATE legal_invitation_jobs SET state=?,error=?,next_attempt_at=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?",
      )
      .bind(
        j.attempts >= 6 ? "failed" : "queued",
        e instanceof Error &&
          ["EMAIL_NOT_CONFIGURED", "PUBLIC_ORIGIN_NOT_CONFIGURED"].includes(
            e.message,
          )
          ? e.message
          : "EMAIL_DELIVERY_FAILED",
        now + Math.min(3600000, 30000 * 2 ** j.attempts),
        j.id,
        token,
      )
      .run();
  }
}
