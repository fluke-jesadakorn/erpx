import "server-only";
import { createHmac } from "node:crypto";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { sendBetaEmail } from "@/server/modules/manage_erpx/public/beta-email";
import type { BetaEnvironment } from "@/server/modules/manage_erpx/public/beta-config";
import { createInvitationDeliveryApplication, type InvitationDeliveryRepository } from "../application/invitation-delivery";
import type { DeliverableInvitation, InvitationDeliveryJob } from "../domain/invitation-delivery";
import { applicationOrigin } from "@/lib/runtime/application-origins";

type DeliveryEnv = BetaEnvironment & { BETTER_AUTH_SECRET?: unknown; ERPX_PUBLIC_ORIGIN?: unknown; BETTER_AUTH_URL?: unknown; CONTROL_DB?: SqlDatabase };
function repository(control: SqlDatabase, holding?: SqlDatabase): InvitationDeliveryRepository {
  return {
    async waiting(holdingId) { return (await control.prepare("SELECT id,invitation_id FROM account_invitation_delivery_jobs WHERE holding_id=? AND state='awaiting_approval' ORDER BY created_at LIMIT 100").bind(holdingId).all<{ id: string; invitation_id: string }>()).results; },
    async review(invitationId) { if (!holding) throw new Error("INVITATION_APPROVAL_REPOSITORY_REQUIRED"); return (await holding.prepare("SELECT status FROM iam_reviews WHERE kind='invitation.authorize' AND payload->>'invitationId'=? ORDER BY created_at DESC LIMIT 1").bind(invitationId).first<{ status: string }>())?.status ?? null; },
    async promote(id, now) { await control.prepare("UPDATE account_invitation_delivery_jobs SET state='queued',next_attempt_at=?,updated_at=? WHERE id=? AND state='awaiting_approval'").bind(now, now, id).run(); },
    async reject(id, now) { await control.prepare("UPDATE account_invitation_delivery_jobs SET state='cancelled',error='INVITATION_REJECTED',updated_at=? WHERE id=? AND state='awaiting_approval'").bind(now, id).run(); },
    async recoverInterrupted(now) { await control.prepare("UPDATE account_invitation_delivery_jobs SET state=CASE WHEN attempts<6 THEN 'queued' ELSE 'failed' END,lease_token=NULL,lease_until=NULL,error='DELIVERY_INTERRUPTED',next_attempt_at=?,updated_at=? WHERE state='running' AND lease_until<?").bind(now, now, now).run(); },
    claim: (lease, now) => control.prepare("UPDATE account_invitation_delivery_jobs SET state='running',attempts=attempts+1,lease_token=?,lease_until=?,updated_at=? WHERE id=(SELECT id FROM account_invitation_delivery_jobs WHERE state='queued' AND next_attempt_at<=? ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING id,invitation_id,attempts,lease_token").bind(lease, now + 30000, now, now).first<InvitationDeliveryJob>(),
    invitation: id => control.prepare("SELECT email,name,status,expires_at FROM account_invitations WHERE id=?").bind(id).first<DeliverableInvitation>(),
    async cancel(id, lease, now) { await control.prepare("UPDATE account_invitation_delivery_jobs SET state='cancelled',lease_token=NULL,lease_until=NULL,error='INVITATION_UNAVAILABLE',updated_at=? WHERE id=? AND lease_token=?").bind(now, id, lease).run(); },
    async sent(id, lease, providerId, now) { await control.prepare("UPDATE account_invitation_delivery_jobs SET state='sent',sent_at=?,provider_id=?,lease_token=NULL,lease_until=NULL,error=NULL,updated_at=? WHERE id=? AND lease_token=?").bind(now, providerId, now, id, lease).run(); },
    async failed(id, lease, failure, now) { await control.prepare("UPDATE account_invitation_delivery_jobs SET state=?,error=?,next_attempt_at=?,lease_token=NULL,lease_until=NULL,updated_at=? WHERE id=? AND lease_token=?").bind(failure.state, failure.code, failure.nextAttemptAt, now, id, lease).run(); },
  };
}
export async function promoteAccountInvitationDeliveries(control: SqlDatabase, holding: SqlDatabase, holdingId: string, now = Date.now()) {
  const application = createInvitationDeliveryApplication({ repository: repository(control, holding), clock: { now: () => now }, identifiers: { next: () => crypto.randomUUID() }, activationLink: () => { throw new Error("DELIVERY_NOT_REQUESTED"); }, send: async () => { throw new Error("DELIVERY_NOT_REQUESTED"); } });
  await application.promote(holdingId);
}
export async function deliverAccountInvitation(control: SqlDatabase, env: DeliveryEnv, send: (env: DeliveryEnv, to: string, subject: string, url: string, key?: string) => Promise<string | null | void> = sendBetaEmail, now = Date.now()) {
  return createInvitationDeliveryApplication({ repository: repository(control), clock: { now: () => now }, identifiers: { next: () => crypto.randomUUID() },
    activationLink(id) { const secret = String(env.BETTER_AUTH_SECRET ?? ""), origin = applicationOrigin("account", String(env.ERPX_PUBLIC_ORIGIN ?? env.BETTER_AUTH_URL ?? "")); if (!secret) throw new Error("INVITATION_SECRET_NOT_CONFIGURED"); if (!origin) throw new Error("PUBLIC_ORIGIN_NOT_CONFIGURED"); const token = createHmac("sha256", secret).update(`invitation:${id}`).digest("hex"); return new URL(`/activate?token=${encodeURIComponent(token)}`, origin).href; },
    send: (to, subject, url, key) => send(env, to, subject, url, key),
  }).deliver();
}
