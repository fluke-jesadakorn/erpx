import type { HoldingId } from "../../runtime/public/runtime";
import type { CustomerPortalSessionActor } from "../domain/portal";

export function createCustomerPortalSessionApplication(ports: { error(status: number, code: string, message: string): Error }) {
  return {
    verify(user: { id: string; email: string; emailVerified: boolean }) {
      if (!user.emailVerified) throw ports.error(403, "CUSTOMER_EMAIL_UNVERIFIED", "Verify your account email before opening customer documents.");
      return { userId: user.id, email: user.email.toLowerCase() };
    },
    actor(user: { userId: string; email: string }, holding: { id: HoldingId; slug: string; displayName: string }): Required<CustomerPortalSessionActor> {
      return { ...user, holdingId: holding.id, holdingSlug: holding.slug, holdingName: holding.displayName };
    },
  };
}
