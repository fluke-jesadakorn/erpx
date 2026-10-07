import "server-only";
import { createHash } from "node:crypto";
import { runtimeEnv } from "../../runtime/public/runtime";
import { ApiError } from "../../runtime/public/http";
import { BusinessRuleError } from "../../shared/domain/error";
import { createIamThrottleApplication } from "../application/throttle";

/** The atomic PostgreSQL counter spans replicas and stores only opaque identity keys. */
export async function iamThrottle(category: string, identity: string, limit = 8, windowMs = 300000) {
  const db = (await runtimeEnv()).CONTROL_DB;
  const application = createIamThrottleApplication({
    async increment(key, now, windowBefore) {
      const row = await db.prepare("INSERT INTO iam_throttles(key,window_start,attempts) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET window_start=CASE WHEN iam_throttles.window_start<? THEN excluded.window_start ELSE iam_throttles.window_start END,attempts=CASE WHEN iam_throttles.window_start<? THEN 1 ELSE iam_throttles.attempts+1 END RETURNING attempts")
        .bind(key, now, windowBefore, windowBefore).first<{ attempts: number }>();
      return row?.attempts ?? null;
    },
  }, { now: () => Date.now() }, (category, identity) => createHash("sha256").update(category).update(":").update(identity).digest("hex"));
  try { await application.check(category, identity, limit, windowMs); }
  catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; }
}
