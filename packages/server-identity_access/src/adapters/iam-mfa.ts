import 'server-only';
import {createCipheriv,createDecipheriv,createHash,createHmac,randomBytes,timingSafeEqual} from 'node:crypto';
import {runtimeEnv} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {iamThrottle} from "@erpx/server-identity_access/public/iam-throttle";
import type {IamActor} from "@erpx/server-identity_access/domain/iam-actor";
import {encodeBase32} from "@erpx/server-identity_access/domain/mfa";
export {encodeBase32} from "@erpx/server-identity_access/domain/mfa";
import {createRequireFreshMfa,createBeginMfa,createVerifyMfa} from "@erpx/server-identity_access/application/mfa";
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
export async function beginMfa(actor:Pick<IamActor,'userId'|'sessionId'>){let env:Awaited<ReturnType<typeof runtimeEnv>>;return createBeginMfa({throttle:actor=>iamThrottle('mfa-enroll',actor.userId,3),enabled:async actor=>{env=await runtimeEnv();return Boolean((await env.CONTROL_DB.prepare(
    "SELECT enabled FROM iam_mfa WHERE user_id=?",
  )
    .bind(actor.userId)
    .first<{ enabled: boolean }>())?.enabled);},sessionCreated:async actor=>(await env.CONTROL_DB.prepare(
    'SELECT "createdAt" FROM session WHERE id=? AND "userId"=?',
  )
    .bind(actor.sessionId, actor.userId)
    .first<{ createdAt: string }>())?.createdAt??null,now:Date.now,error,secret:()=>encodeBase32(randomBytes(20)),seal:sealSecret,enroll:async(actor,sealed)=>{  await env.CONTROL_DB.prepare(
    "INSERT INTO iam_mfa(user_id,secret) VALUES(?,?) ON CONFLICT(user_id) DO UPDATE SET secret=excluded.secret WHERE iam_mfa.enabled=false",
  )
    .bind(actor.userId, sealed)
    .run();
}})(actor);}
export async function verifyMfa(actor:Pick<IamActor,'userId'|'sessionId'>,code:string){let env:Awaited<ReturnType<typeof runtimeEnv>>;return createVerifyMfa({throttle:actor=>iamThrottle('mfa',actor.userId),enrollment:async actor=>{env=await runtimeEnv();return await env.CONTROL_DB.prepare(
    "SELECT secret,last_counter FROM iam_mfa WHERE user_id=?",
  )
    .bind(actor.userId)
    .first<{ secret: string; last_counter: number }>();},now:Date.now,error,open:openSecret,matches:(secret,counter,code)=>timingSafeEqual(Buffer.from(totp(secret,counter)),Buffer.from(code)),activate:async(actor,counter)=>Boolean(await env.CONTROL_DB.prepare(
    "UPDATE iam_mfa SET enabled=true,last_counter=? WHERE user_id=? AND last_counter<? RETURNING user_id",
  )
    .bind(counter, actor.userId, counter)
    .first()),verified:async(actor,now)=>{  await env.CONTROL_DB.prepare(
    "INSERT INTO iam_mfa_sessions VALUES(?,?,?) ON CONFLICT(session_id) DO UPDATE SET verified_at=excluded.verified_at",
  )
    .bind(actor.sessionId, actor.userId, now)
    .run();
}})(actor,code);}
