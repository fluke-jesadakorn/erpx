import { nativePrepare } from '@erpx/server-identity_access/public/native-identity-write';
import "server-only";
import { createHash } from "node:crypto";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { BusinessRuleError } from "@erpx/server-shared/domain/error";
import { createIamThrottleApplication } from "@erpx/server-identity_access/application/throttle";

/** The atomic PostgreSQL counter spans replicas and stores only opaque identity keys. */
export async function iamThrottle(category: string, identity: string, limit = 8, windowMs = 300000) {
  const db = (await runtimeEnv()).CONTROL_DB;
  const application = createIamThrottleApplication({
    async increment(key, now, windowBefore) {
      const row = await nativePrepare("w_a2e9726082b9")
        .bind(key, now, windowBefore, windowBefore).first<{ attempts: number }>();
      return row?.attempts ?? null;
    },
  }, { now: () => Date.now() }, (category, identity) => createHash("sha256").update(category).update(":").update(identity).digest("hex"));
  try { await application.check(category, identity, limit, windowMs); }
  catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; }
}
