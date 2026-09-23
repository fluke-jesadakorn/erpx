import "server-only";
import {
  createCipheriv,
  createDecipheriv,
  createHash,
  createHmac,
  randomBytes,
} from "node:crypto";
import type { User } from "better-auth";
import type { BetaEnvironment } from "./beta-config";
import { sendBetaEmail } from "./beta-email";
import type { SqlDatabase } from "./runtime/types";

type RecoveryEnv = BetaEnvironment & {
  BETTER_AUTH_SECRET: string;
  CONTROL_DB?: SqlDatabase;
};
type RecoverySender = (
  env: RecoveryEnv,
  to: string,
  subject: string,
  url: string,
  key?: string,
) => Promise<string | null | void>;

function encryptionKey(secret: string) {
  return createHash("sha256")
    .update(secret)
    .update("erpx-account-recovery-v1")
    .digest();
}
function seal(secret: string, value: string) {
  const iv = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", encryptionKey(secret), iv);
  const data = Buffer.concat([cipher.update(value, "utf8"), cipher.final()]);
  return [iv, cipher.getAuthTag(), data]
    .map((part) => part.toString("base64url"))
    .join(".");
}
function open(secret: string, value: string) {
  const [iv, tag, data] = value
    .split(".")
    .map((part) => Buffer.from(part, "base64url"));
  const decipher = createDecipheriv(
    "aes-256-gcm",
    encryptionKey(secret),
    iv,
  );
  decipher.setAuthTag(tag);
  return Buffer.concat([decipher.update(data), decipher.final()]).toString(
    "utf8",
  );
}
function event(
  control: SqlDatabase,
  userId: string,
  jobId: string | null,
  action: string,
  evidence: Record<string, unknown>,
  now: number,
) {
  return control
    .prepare(
      "INSERT INTO account_recovery_events(id,user_id,job_id,action,evidence,created_at) VALUES(?,?,?,?,?::jsonb,?)",
    )
    .bind(
      crypto.randomUUID(),
      userId,
      jobId,
      action,
      JSON.stringify(evidence),
      now,
    );
}

export async function queueAccountRecovery(
  control: SqlDatabase,
  env: RecoveryEnv,
  user: User,
  url: string,
  token: string,
  now = Date.now(),
) {
  const verificationIdentifier = `reset-password:${token}`;
  const id = `account-recovery-${createHmac("sha256", env.BETTER_AUTH_SECRET)
    .update(token)
    .digest("hex")
    .slice(0, 40)}`;
  await control.batch([
    control
      .prepare(
        "DELETE FROM verification WHERE value=? AND identifier LIKE 'reset-password:%' AND identifier!=?",
      )
      .bind(user.id, verificationIdentifier),
    control
      .prepare(
        "UPDATE account_recovery_delivery_jobs SET state='cancelled',error='SUPERSEDED',lease_token=NULL,lease_until=NULL,updated_at=? WHERE user_id=? AND state IN ('queued','running','failed')",
      )
      .bind(now, user.id),
    control
      .prepare(
        "INSERT INTO account_recovery_delivery_jobs(id,user_id,verification_identifier,url_ciphertext,state,next_attempt_at,expires_at,created_at,updated_at) VALUES(?,?,?,?,'queued',?,?,?,?)",
      )
      .bind(
        id,
        user.id,
        verificationIdentifier,
        seal(env.BETTER_AUTH_SECRET, url),
        now,
        now + 3600000,
        now,
        now,
      ),
    event(control, user.id, id, "requested", { expiresAt: now + 3600000 }, now),
  ]);
}

export async function deliverAccountRecovery(
  control: SqlDatabase,
  env: RecoveryEnv,
  send: RecoverySender = sendBetaEmail,
  now = Date.now(),
) {
  await control
    .prepare(
      "UPDATE account_recovery_delivery_jobs SET state=CASE WHEN attempts<6 THEN 'queued' ELSE 'failed' END,lease_token=NULL,lease_until=NULL,error='DELIVERY_INTERRUPTED',next_attempt_at=?,updated_at=? WHERE state='running' AND lease_until<?",
    )
    .bind(now, now, now)
    .run();
  const lease = crypto.randomUUID();
  const job = await control
    .prepare(
      "UPDATE account_recovery_delivery_jobs SET state='running',attempts=attempts+1,lease_token=?,lease_until=?,updated_at=? WHERE id=(SELECT id FROM account_recovery_delivery_jobs WHERE state='queued' AND next_attempt_at<=? ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING id,user_id,url_ciphertext,attempts,expires_at",
    )
    .bind(lease, now + 30000, now, now)
    .first<{
      id: string;
      user_id: string;
      url_ciphertext: string;
      attempts: number;
      expires_at: number;
    }>();
  if (!job) return false;
  const account = await control
    .prepare('SELECT email FROM "user" WHERE id=?')
    .bind(job.user_id)
    .first<{ email: string }>();
  const cancel = async (reason: string) => {
    await control.batch([
      control
        .prepare(
          "UPDATE account_recovery_delivery_jobs SET state='cancelled',error=?,lease_token=NULL,lease_until=NULL,updated_at=? WHERE id=? AND lease_token=?",
        )
        .bind(reason, now, job.id, lease),
      event(control, job.user_id, job.id, "cancelled", { reason }, now),
    ]);
  };
  if (!account || job.expires_at <= now) {
    await cancel(account ? "RECOVERY_EXPIRED" : "ACCOUNT_UNAVAILABLE");
    return true;
  }
  try {
    const providerId = await send(
      env,
      account.email,
      "Reset your ERPX password",
      open(env.BETTER_AUTH_SECRET, job.url_ciphertext),
      job.id,
    );
    await control.batch([
      control
        .prepare(
          "UPDATE account_recovery_delivery_jobs SET state='sent',sent_at=?,provider_id=?,lease_token=NULL,lease_until=NULL,error=NULL,updated_at=? WHERE id=? AND lease_token=?",
        )
        .bind(
          now,
          typeof providerId === "string" ? providerId : job.id,
          now,
          job.id,
          lease,
        ),
      event(
        control,
        job.user_id,
        job.id,
        "sent",
        { providerId: typeof providerId === "string" ? providerId : null },
        now,
      ),
    ]);
  } catch (error) {
    const code =
      error instanceof Error && error.message === "EMAIL_NOT_CONFIGURED"
        ? error.message
        : "EMAIL_DELIVERY_FAILED";
    const failed = job.attempts >= 6;
    await control.batch([
      control
        .prepare(
          "UPDATE account_recovery_delivery_jobs SET state=?,error=?,next_attempt_at=?,lease_token=NULL,lease_until=NULL,updated_at=? WHERE id=? AND lease_token=?",
        )
        .bind(
          failed ? "failed" : "queued",
          code,
          now + Math.min(3600000, 30000 * 2 ** job.attempts),
          now,
          job.id,
          lease,
        ),
      event(
        control,
        job.user_id,
        job.id,
        "delivery_failed",
        { attempt: job.attempts, terminal: failed, error: code },
        now,
      ),
    ]);
  }
  return true;
}

export async function recordAccountRecoveryCompleted(
  control: SqlDatabase,
  userId: string,
  now = Date.now(),
) {
  const latest = await control
    .prepare(
      "SELECT id FROM account_recovery_delivery_jobs WHERE user_id=? ORDER BY created_at DESC,id DESC LIMIT 1",
    )
    .bind(userId)
    .first<string>("id");
  await control.batch([
    control
      .prepare('DELETE FROM session WHERE "userId"=?')
      .bind(userId),
    control
      .prepare(
        "UPDATE account_recovery_delivery_jobs SET state='cancelled',error='PASSWORD_ALREADY_RESET',lease_token=NULL,lease_until=NULL,updated_at=? WHERE user_id=? AND state IN ('queued','running','failed')",
      )
      .bind(now, userId),
    event(control, userId, latest ?? null, "completed", { sessionsRevoked: true }, now),
  ]);
}
