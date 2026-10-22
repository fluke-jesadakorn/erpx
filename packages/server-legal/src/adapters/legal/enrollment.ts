import "server-only";
import { SignJWT, jwtVerify } from "jose";
import { runtimeEnv, holdingBindingBySlug, holdingResources } from "../../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { createLegalEnrollmentAccess } from "@erpx/server-legal/application/enrollment";
import { legalEnrollmentAudience } from "@erpx/server-legal/domain/enrollment";
import {legalPortalPurchasePredicate} from '@erpx/server-legal/adapters/portal-purchases';
export function enrollmentToken(secret: string, slug: string, shareId: string, expiresAt: number) { return new SignJWT({ slug, shareId }).setProtectedHeader({ alg: "HS256" }).setAudience(legalEnrollmentAudience).setIssuedAt().setExpirationTime(Math.floor(expiresAt / 1000)).sign(new TextEncoder().encode(secret)); }
export async function enrollmentAccess(token: string) {
  const env = await runtimeEnv();
  return createLegalEnrollmentAccess({ verify: async token => (await jwtVerify(token, new TextEncoder().encode(env.BETTER_AUTH_SECRET), { algorithms: ["HS256"], audience: legalEnrollmentAudience })).payload, clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), load: async slug => {
    const binding = await holdingBindingBySlug(slug), { write } = await holdingResources(binding.id);
    return { resource: { write, env }, share: async (id, now) => write.prepare(`SELECT s.email,s.id FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id JOIN legal_settings c ON c.company_id=i.company_id AND c.enabled WHERE s.id=? AND s.user_id IS NULL AND s.revoked_at IS NULL AND s.expires_at>? AND ${await legalPortalPurchasePredicate('i')} AND NOT EXISTS(SELECT 1 FROM legal_destructions destruction WHERE destruction.item_id=i.id AND destruction.state IN ('executing','completed'))`).bind(id, now).first<{ email: string; id: string }>() };
  } })(token);
}
