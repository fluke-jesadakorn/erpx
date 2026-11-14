import 'server-only';
import {createCipheriv,createDecipheriv,createHash,createHmac,randomBytes} from 'node:crypto';
import {runtimeEnv} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {IamActor} from "@erpx/server-identity_access/domain/iam-actor";
export {encodeBase32} from "@erpx/server-identity_access/domain/mfa";
import {createRequireFreshMfa} from "@erpx/server-identity_access/application/mfa";
const alphabet='ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
const error=(status:number,code:string,message:string)=>new ApiError(status,code,message);
export function totp(secret: string, counter: number) {
  let bits = 0,
    value = 0;
  const bytes = [];
  for (const char of secret) {
    const n = alphabet.indexOf(char);
    if (n < 0) throw new Error("Invalid MFA secret");
    value = (value << 5) | n;
    bits += 5;
    if (bits >= 8) {
      bytes.push((value >>> (bits - 8)) & 255);
      bits -= 8;
    }
  }
  const input = Buffer.alloc(8);
  input.writeBigUInt64BE(BigInt(counter));
  const hash = createHmac("sha1", Buffer.from(bytes)).update(input).digest(),
    offset = hash[19] & 15;
  return String((hash.readUInt32BE(offset) & 0x7fffffff) % 1000000).padStart(
    6,
    "0",
  );
}
async function secretKey() {
  return createHash("sha256")
    .update((await runtimeEnv()).BETTER_AUTH_SECRET)
    .update("erpx-iam-mfa-v1")
    .digest();
}
export async function sealSecret(secret: string) {
  const iv = randomBytes(12),
    cipher = createCipheriv("aes-256-gcm", await secretKey(), iv),
    data = Buffer.concat([cipher.update(secret, "utf8"), cipher.final()]);
  return [iv, cipher.getAuthTag(), data]
    .map((b) => b.toString("base64url"))
    .join(".");
}
export async function openSecret(secret: string) {
  const [iv, tag, data] = secret
    .split(".")
    .map((s) => Buffer.from(s, "base64url"));
  const cipher = createDecipheriv("aes-256-gcm", await secretKey(), iv);
  cipher.setAuthTag(tag);
  return Buffer.concat([cipher.update(data), cipher.final()]).toString("utf8");
}
export async function requireFreshMfa(actor:Pick<IamActor,'userId'|'sessionId'>){let env:Awaited<ReturnType<typeof runtimeEnv>>;return createRequireFreshMfa({ready:async()=>{env=await runtimeEnv();},now:Date.now,isoNow:()=>new Date().toISOString(),error,verified:async(actor,cutoff,isoNow)=>Boolean(await env.CONTROL_DB.prepare(
    'SELECT 1 FROM iam_mfa_sessions f JOIN iam_mfa m ON m.user_id=f.user_id AND m.enabled=true JOIN session s ON s.id=f.session_id AND s."userId"=f.user_id WHERE f.user_id=? AND f.session_id=? AND f.verified_at>? AND s."expiresAt"::timestamptz>?::timestamptz',
  )
    .bind(
      actor.userId,
      actor.sessionId,
      cutoff,
      isoNow,
    )
    .first())})(actor);}
