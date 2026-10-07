import type { Clock } from "../../shared/application/ports";
import { legalEnrollmentClaims } from "../domain/enrollment";
export function createLegalEnrollmentAccess<Resource extends object>(ports: {
  verify(token: string): Promise<unknown>;
  load(slug: string): Promise<{ resource: Resource; share(id: string, now: number): Promise<{ email: string; id: string } | null> }>;
  clock: Clock; error(status: number, code: string, message: string): Error;
}) {
  return async (token: string) => {
    let payload;
    try { payload = await ports.verify(token); } catch { throw ports.error(404, "LEGAL_INVITATION_UNAVAILABLE", "Invitation expired or unavailable."); }
    const input = legalEnrollmentClaims.parse(payload), context = await ports.load(input.slug), share = await context.share(input.shareId, ports.clock.now());
    if (!share) throw ports.error(404, "LEGAL_INVITATION_UNAVAILABLE", "Invitation expired or already accepted.");
    return { ...context.resource, slug: input.slug, ...share };
  };
}
