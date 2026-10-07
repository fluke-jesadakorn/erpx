import type { Clock, IdentifierSource } from "../../shared/application/ports";
export type LegalInvitationJob = { id: string; share_id: string; holding_slug: string; attempts: number };
export type LegalInvitationRepository = {
  recover(now: number): Promise<void>; claim(token: string, now: number): Promise<LegalInvitationJob | null>;
  share(id: string, now: number): Promise<{ email: string; expires_at: number } | null>;
  cancel(id: string, token: string): Promise<void>; sent(id: string, token: string, now: number): Promise<void>;
  failed(id: string, token: string, state: "failed" | "queued", error: string, nextAttempt: number): Promise<void>;
};
export function createLegalInvitationWorker(ports: { repository: LegalInvitationRepository; clock: Clock; identifiers: IdentifierSource; publicOrigin?: string; enrollmentEnabled: boolean; enrollmentToken(slug: string, shareId: string, expiresAt: number): Promise<string>; send(email: string, subject: string, body: string, key: string): Promise<void> }) {
  return async () => {
    const now = ports.clock.now(); await ports.repository.recover(now);
    const token = ports.identifiers.next(), job = await ports.repository.claim(token, now); if (!job) return;
    try {
      const share = await ports.repository.share(job.share_id, ports.clock.now());
      if (!share) { await ports.repository.cancel(job.id, token); return; }
      if (!ports.publicOrigin) throw new Error("PUBLIC_ORIGIN_NOT_CONFIGURED");
      const url = new URL(`/legal-portal/${encodeURIComponent(job.holding_slug)}`, ports.publicOrigin).href;
      const inviteLink = ports.enrollmentEnabled ? new URL(`/legal-portal/enroll#token=${await ports.enrollmentToken(job.holding_slug, job.share_id, share.expires_at)}`, ports.publicOrigin).href : null;
      await ports.send(share.email, "You have an ERPX Legal document invitation", inviteLink ? `${url}\n\nNew invited account: ${inviteLink}` : url, `legal-invitation-${job.id}`);
      await ports.repository.sent(job.id, token, ports.clock.now());
    } catch (error) {
      const code = error instanceof Error && ["EMAIL_NOT_CONFIGURED", "PUBLIC_ORIGIN_NOT_CONFIGURED"].includes(error.message) ? error.message : "EMAIL_DELIVERY_FAILED";
      await ports.repository.failed(job.id, token, job.attempts >= 6 ? "failed" : "queued", code, now + Math.min(3600000, 30000 * 2 ** job.attempts));
    }
  };
}
