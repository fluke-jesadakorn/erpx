import "server-only";
import { SignJWT, jwtVerify } from "jose";
import { runtimeEnv, holdingBindingBySlug, holdingResources } from "../../../../../../platform/runtime/server/src/public/runtime";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { createLegalEnrollmentAccess } from "@erpx/legal-server/application/enrollment";
import { legalEnrollmentAudience } from "@erpx/legal-server/domain/enrollment";
import {legalPortalPurchasePredicate,legalActiveDestructionGuard} from '@erpx/legal-server/adapters/portal-purchases';
import {legalShareDocumentReadHead} from '../legal-read-tail-r41-sources';
export function enrollmentToken(secret: string, slug: string, shareId: string, expiresAt: number) { return new SignJWT({ slug, shareId }).setProtectedHeader({ alg: "HS256" }).setAudience(legalEnrollmentAudience).setIssuedAt().setExpirationTime(Math.floor(expiresAt / 1000)).sign(new TextEncoder().encode(secret)); }
export async function enrollmentAccess(token: string) {
  const env = await runtimeEnv();
  return createLegalEnrollmentAccess({ verify: async token => (await jwtVerify(token, new TextEncoder().encode(env.BETTER_AUTH_SECRET), { algorithms: ["HS256"], audience: legalEnrollmentAudience })).payload, clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), load: async slug => {
    const binding = await holdingBindingBySlug(slug), { write } = await holdingResources(binding.id);
    return { resource: { write, env }, share: async (id, now) => write.prepare(`${legalShareDocumentReadHead('s.email,s.id')}JOIN legal_settings c ON c.company_id=i.company_id AND c.enabled WHERE s.id=? AND s.user_id IS NULL AND s.revoked_at IS NULL AND s.expires_at>? AND ${await legalPortalPurchasePredicate('i')} ${legalActiveDestructionGuard()}`).bind(id, now).first<{ email: string; id: string }>() };
  } })(token);
}
