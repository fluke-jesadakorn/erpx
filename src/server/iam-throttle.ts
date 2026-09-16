import "server-only";
import { createHash } from "node:crypto";
import { runtimeEnv } from "./runtime";
import { ApiError } from "./http";
/** Database backed across replicas; stores opaque keys, never supplied passwords. */
export async function iamThrottle(
  category: string,
  identity: string,
  limit = 8,
  windowMs = 300000,
) {
  const db = (await runtimeEnv()).CONTROL_DB,
    now = Date.now(),
    key = createHash("sha256")
      .update(category)
      .update(":")
      .update(identity.toLowerCase())
      .digest("hex");
  const row = await db
    .prepare(
      "INSERT INTO iam_throttles(key,window_start,attempts) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET window_start=CASE WHEN iam_throttles.window_start<? THEN excluded.window_start ELSE iam_throttles.window_start END,attempts=CASE WHEN iam_throttles.window_start<? THEN 1 ELSE iam_throttles.attempts+1 END RETURNING attempts",
    )
    .bind(key, now, now - windowMs, now - windowMs)
    .first<{ attempts: number }>();
  if (!row || row.attempts > limit)
    throw new ApiError(
      429,
      "IAM_RATE_LIMIT",
      "Too many attempts. Wait five minutes before trying again.",
    );
}
