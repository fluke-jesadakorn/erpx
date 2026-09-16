import { iamThrottle } from "./iam-throttle";
import "server-only";
import {
  createCipheriv,
  createDecipheriv,
  createHash,
  createHmac,
  randomBytes,
  timingSafeEqual,
} from "node:crypto";
import { runtimeEnv } from "./runtime";
import { ApiError } from "./http";
import type { IamActor } from "./iam-context";
const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
export function encodeBase32(bytes: Uint8Array) {
  let bits = 0,
    value = 0,
    out = "";
  for (const byte of bytes) {
    value = (value << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      out += alphabet[(value >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits) out += alphabet[(value << (5 - bits)) & 31];
  return out;
}
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
export async function requireFreshMfa(
  actor: Pick<IamActor, "userId" | "sessionId">,
) {
  const env = await runtimeEnv();
  const row = await env.CONTROL_DB.prepare(
    'SELECT 1 FROM iam_mfa_sessions f JOIN iam_mfa m ON m.user_id=f.user_id AND m.enabled=true JOIN session s ON s.id=f.session_id AND s."userId"=f.user_id WHERE f.user_id=? AND f.session_id=? AND f.verified_at>? AND s."expiresAt"::timestamptz>?::timestamptz',
  )
    .bind(
      actor.userId,
      actor.sessionId,
      Date.now() - 300_000,
      new Date().toISOString(),
    )
    .first();
  if (!row)
    throw new ApiError(
      403,
      "FRESH_MFA_REQUIRED",
      "Verify an authenticator code in My access before this sensitive change.",
    );
}
export async function beginMfa(actor: Pick<IamActor, "userId" | "sessionId">) {
  await iamThrottle("mfa-enroll", actor.userId, 3);
  const env = await runtimeEnv();
  const current = await env.CONTROL_DB.prepare(
    "SELECT enabled FROM iam_mfa WHERE user_id=?",
  )
    .bind(actor.userId)
    .first<{ enabled: boolean }>();
  if (current?.enabled)
    throw new ApiError(
      409,
      "MFA_ALREADY_ENABLED",
      "Your authenticator is already configured.",
    );
  const session = await env.CONTROL_DB.prepare(
    'SELECT "createdAt" FROM session WHERE id=? AND "userId"=?',
  )
    .bind(actor.sessionId, actor.userId)
    .first<{ createdAt: string }>();
  if (
    !session ||
    !Number.isFinite(Date.parse(session.createdAt)) ||
    Date.parse(session.createdAt) < Date.now() - 300_000
  )
    throw new ApiError(
      403,
      "REAUTHENTICATION_REQUIRED",
      "Sign in again before enrolling an authenticator.",
    );
  const secret = encodeBase32(randomBytes(20));
  await env.CONTROL_DB.prepare(
    "INSERT INTO iam_mfa(user_id,secret) VALUES(?,?) ON CONFLICT(user_id) DO UPDATE SET secret=excluded.secret WHERE iam_mfa.enabled=false",
  )
    .bind(actor.userId, await sealSecret(secret))
    .run();
  return {
    secret,
    uri: `otpauth://totp/ERPX:${encodeURIComponent(actor.userId)}?secret=${secret}&issuer=ERPX&algorithm=SHA1&digits=6&period=30`,
  };
}
export async function verifyMfa(
  actor: Pick<IamActor, "userId" | "sessionId">,
  code: string,
) {
  await iamThrottle("mfa", actor.userId);
  if (!/^\d{6}$/.test(code))
    throw new ApiError(
      422,
      "MFA_INVALID",
      "Enter a six-digit authenticator code.",
    );
  const env = await runtimeEnv();
  const row = await env.CONTROL_DB.prepare(
    "SELECT secret,last_counter FROM iam_mfa WHERE user_id=?",
  )
    .bind(actor.userId)
    .first<{ secret: string; last_counter: number }>();
  if (!row)
    throw new ApiError(
      409,
      "MFA_NOT_ENROLLED",
      "Set up your authenticator first.",
    );
  const now = Math.floor(Date.now() / 30000),
    secret = await openSecret(row.secret);
  const counter = [now - 1, now, now + 1].find(
    (c) =>
      c > Number(row.last_counter) &&
      timingSafeEqual(Buffer.from(totp(secret, c)), Buffer.from(code)),
  );
  if (counter === undefined)
    throw new ApiError(
      403,
      "MFA_INVALID",
      "The code is invalid or already used.",
    );
  const result = await env.CONTROL_DB.prepare(
    "UPDATE iam_mfa SET enabled=true,last_counter=? WHERE user_id=? AND last_counter<? RETURNING user_id",
  )
    .bind(counter, actor.userId, counter)
    .first();
  if (!result)
    throw new ApiError(403, "MFA_REPLAY", "This code was already used.");
  await env.CONTROL_DB.prepare(
    "INSERT INTO iam_mfa_sessions VALUES(?,?,?) ON CONFLICT(session_id) DO UPDATE SET verified_at=excluded.verified_at",
  )
    .bind(actor.sessionId, actor.userId, Date.now())
    .run();
  return { verifiedUntil: Date.now() + 300_000 };
}
