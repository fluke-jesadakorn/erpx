import { createHash, createHmac, timingSafeEqual } from "node:crypto";
import {
  customerPortalInvitationAudience,
  customerPortalInvitationClaimsSchema,
  type CustomerPortalInvitationClaims,
  type CustomerPortalInvitationTokenFacts,
} from "@erpx/server-customer_authority/domain/portal-invitation";

function signingKey(secret: string) {
  return createHmac("sha256", secret).update(`${customerPortalInvitationAudience}:signing-key`).digest();
}

export function customerPortalInvitationToken(secret: string, facts: CustomerPortalInvitationTokenFacts) {
  const claims = customerPortalInvitationClaimsSchema.parse(facts), encoded = Buffer.from(JSON.stringify(claims)).toString("base64url"),
    signature = createHmac("sha256", signingKey(secret)).update(encoded).digest("base64url");
  return `${encoded}.${signature}`;
}

export function customerPortalInvitationTokenHash(token: string) {
  return createHash("sha256").update(token).digest("hex");
}

export function customerPortalInvitationTokenHashMatches(token: string, expectedHash: string) {
  if (!/^[a-f0-9]{64}$/.test(expectedHash)) return false;
  const actual = createHash("sha256").update(token).digest(), expected = Buffer.from(expectedHash,"hex");
  return timingSafeEqual(actual,expected);
}

export function verifyCustomerPortalInvitationToken(secret: string, token: string): CustomerPortalInvitationClaims | null {
  if (!token || token.length > 2048 || !/^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/.test(token)) return null;
  const [encoded, provided] = token.split("."), expected = createHmac("sha256", signingKey(secret)).update(encoded).digest();
  let supplied: Buffer;
  try { supplied = Buffer.from(provided, "base64url"); } catch { return null; }
  if (supplied.length !== expected.length || !timingSafeEqual(supplied, expected)) return null;
  try {
    const claims = customerPortalInvitationClaimsSchema.safeParse(JSON.parse(Buffer.from(encoded, "base64url").toString("utf8")));
    return claims.success ? claims.data : null;
  } catch { return null; }
}
