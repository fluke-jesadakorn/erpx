import "server-only";
import { SignJWT, jwtVerify } from "jose";
import { z } from "zod";
import { runtimeEnv, holdingBindingBySlug, holdingResources } from "../runtime";
import { fail } from "./access";
const audience = "erpx-legal-enrollment";
export async function enrollmentToken(
  secret: string,
  slug: string,
  shareId: string,
  expiresAt: number,
) {
  return new SignJWT({ slug, shareId })
    .setProtectedHeader({ alg: "HS256" })
    .setAudience(audience)
    .setIssuedAt()
    .setExpirationTime(Math.floor(expiresAt / 1000))
    .sign(new TextEncoder().encode(secret));
}
export async function enrollmentAccess(token: string) {
  const env = await runtimeEnv();
  let payload;
  try {
    payload = (
      await jwtVerify(token, new TextEncoder().encode(env.BETTER_AUTH_SECRET), {
        algorithms: ["HS256"],
        audience,
      })
    ).payload;
  } catch {
    fail(
      "Invitation expired or unavailable.",
      "LEGAL_INVITATION_UNAVAILABLE",
      404,
    );
  }
  const x = z.object({ slug: z.string(), shareId: z.string() }).parse(payload),
    binding = await holdingBindingBySlug(x.slug),
    { write } = await holdingResources(binding.id),
    share = await write
      .prepare(
        "SELECT s.email,s.id FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_settings c ON c.company_id=i.company_id AND c.enabled WHERE s.id=? AND s.user_id IS NULL AND s.revoked_at IS NULL AND s.expires_at>?",
      )
      .bind(x.shareId, Date.now())
      .first<{ email: string; id: string }>();
  if (!share)
    fail(
      "Invitation expired or already accepted.",
      "LEGAL_INVITATION_UNAVAILABLE",
      404,
    );
  return { write, env, slug: x.slug, ...share };
}
