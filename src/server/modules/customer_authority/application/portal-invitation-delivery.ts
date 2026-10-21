import type { Clock, IdentifierSource } from "../../shared/application/ports";
import type { CustomerPortalInvitationTokenFacts } from "../domain/portal-invitation";

export type CustomerPortalInvitationDeliveryJob = {
  id: string;
  binding_id: string;
  holding_slug: string;
  generation: number;
  email: string;
  token_hash: string;
  token_key_version: 1;
  token_expires_at: number;
  attempts: number;
  lease_token: string;
};
export type CustomerPortalInvitationDeliveryRepository = {
  recover(now: number): Promise<void>;
  claim(token: string, now: number): Promise<CustomerPortalInvitationDeliveryJob | null>;
  current(job: CustomerPortalInvitationDeliveryJob, now: number): Promise<boolean>;
  cancel(job: CustomerPortalInvitationDeliveryJob, now: number, state: "revoked" | "expired", reason: string): Promise<void>;
  sent(job: CustomerPortalInvitationDeliveryJob, now: number): Promise<void>;
  failed(job: CustomerPortalInvitationDeliveryJob, now: number, state: "failed" | "queued", code: string, nextAttempt: number): Promise<void>;
};

export function createCustomerPortalInvitationDelivery(ports: {
  repository: CustomerPortalInvitationDeliveryRepository;
  clock: Clock;
  identifiers: IdentifierSource;
  token(facts: CustomerPortalInvitationTokenFacts): string;
  tokenHashMatches(token: string, expectedHash: string): boolean;
  accountOrigin?: string;
  send(to: string, subject: string, body: string, key: string, sensitive: true): Promise<void>;
}) {
  return async () => {
    const now = ports.clock.now();
    await ports.repository.recover(now);
    const leaseToken = ports.identifiers.next(), job = await ports.repository.claim(leaseToken, now);
    if (!job) return false;
    try {
      if (Number(job.token_expires_at) <= ports.clock.now()) {
        await ports.repository.cancel(job,ports.clock.now(),"expired","INVITATION_EXPIRED");
        return true;
      }
      if (!await ports.repository.current(job,ports.clock.now())) {
        await ports.repository.cancel(job,ports.clock.now(),"revoked","SOURCE_UNAVAILABLE");
        return true;
      }
      if (!ports.accountOrigin) throw new Error("PUBLIC_ORIGIN_NOT_CONFIGURED");
      const facts: CustomerPortalInvitationTokenFacts = {
        invitationId: job.id,
        bindingId: job.binding_id,
        holdingSlug: job.holding_slug,
        generation: Number(job.generation),
        expiresAt: Number(job.token_expires_at),
        keyVersion: Number(job.token_key_version) as 1,
      };
      const token = ports.token(facts);
      if (!ports.tokenHashMatches(token,job.token_hash)) throw new Error("INVITATION_SIGNING_SECRET_CHANGED");
      const url = new URL("/customer-portal/enroll",ports.accountOrigin);
      url.hash = new URLSearchParams({token}).toString();
      await ports.send(job.email,"Your ERPX Customer Portal invitation",`Use this private link to verify your account and accept the invitation for the approved customer document.\n\n${url.href}\n\nThe link expires at ${new Date(Number(job.token_expires_at)).toISOString()}. After verifying your email, reopen this invitation link and sign in to accept it.`,`customer-portal-invitation-${job.id}`,true);
      await ports.repository.sent(job,ports.clock.now());
    } catch (error) {
      const code = error instanceof Error && ["EMAIL_NOT_CONFIGURED","PUBLIC_ORIGIN_NOT_CONFIGURED","INVITATION_SIGNING_SECRET_UNAVAILABLE","INVITATION_SIGNING_SECRET_CHANGED"].includes(error.message)
        ? error.message
        : "EMAIL_DELIVERY_FAILED";
      await ports.repository.failed(job,ports.clock.now(),Number(job.attempts)>=6?"failed":"queued",code,now+Math.min(3600000,30000*2**Number(job.attempts)));
    }
    return true;
  };
}
