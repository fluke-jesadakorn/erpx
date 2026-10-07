import type { Clock, IdentifierSource } from "../../shared";
import { invitationCanBeDelivered, invitationDeliveryFailure, type DeliverableInvitation, type InvitationDeliveryJob } from "../domain/invitation-delivery";

export interface InvitationDeliveryRepository {
  waiting(holdingId: string): Promise<{ id: string; invitation_id: string }[]>;
  review(invitationId: string): Promise<string | null>;
  promote(jobId: string, now: number): Promise<void>;
  reject(jobId: string, now: number): Promise<void>;
  recoverInterrupted(now: number): Promise<void>;
  claim(lease: string, now: number): Promise<InvitationDeliveryJob | null>;
  invitation(invitationId: string): Promise<DeliverableInvitation | null>;
  cancel(jobId: string, lease: string, now: number): Promise<void>;
  sent(jobId: string, lease: string, providerId: string, now: number): Promise<void>;
  failed(jobId: string, lease: string, failure: ReturnType<typeof invitationDeliveryFailure>, now: number): Promise<void>;
}
export interface InvitationDeliveryPorts {
  repository: InvitationDeliveryRepository;
  clock: Clock;
  identifiers: IdentifierSource;
  activationLink(invitationId: string): string;
  send(to: string, subject: string, url: string, key: string): Promise<string | null | void>;
}

export function createInvitationDeliveryApplication(ports: InvitationDeliveryPorts) {
  return {
    async promote(holdingId: string): Promise<void> {
      const now = ports.clock.now();
      for (const job of await ports.repository.waiting(holdingId)) {
        const review = await ports.repository.review(job.invitation_id);
        if (review === "approved") await ports.repository.promote(job.id, now);
        else if (review === "rejected") await ports.repository.reject(job.id, now);
      }
    },
    async deliver(): Promise<boolean> {
      const now = ports.clock.now();
      await ports.repository.recoverInterrupted(now);
      const lease = ports.identifiers.next();
      const job = await ports.repository.claim(lease, now);
      if (!job) return false;
      try {
        const invitation = await ports.repository.invitation(job.invitation_id);
        if (!invitationCanBeDelivered(invitation, now)) { await ports.repository.cancel(job.id, lease, now); return true; }
        const providerId = await ports.send(invitation.email, "Activate your ERPX account", ports.activationLink(job.invitation_id), job.id);
        await ports.repository.sent(job.id, lease, typeof providerId === "string" ? providerId : job.id, now);
      } catch (error) {
        await ports.repository.failed(job.id, lease, invitationDeliveryFailure(error, job.attempts, now), now);
      }
      return true;
    },
  };
}
