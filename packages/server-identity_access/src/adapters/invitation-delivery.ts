import { nativePrepare } from '@erpx/server-identity_access/public/native-identity-write';
import "server-only";
import { createHmac } from "node:crypto";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { sendBetaEmail } from "@erpx/server-manage_erpx/public/beta-email";
import type { BetaEnvironment } from "@erpx/server-manage_erpx/public/beta-config";
import { createInvitationDeliveryApplication, type InvitationDeliveryRepository } from "@erpx/server-identity_access/application/invitation-delivery";
import type { DeliverableInvitation, InvitationDeliveryJob } from "@erpx/server-identity_access/domain/invitation-delivery";
import { applicationOrigin } from "@erpx/feature-runtime/lib/application-origins";

type DeliveryEnv = BetaEnvironment & { BETTER_AUTH_SECRET?: unknown; ERPX_PUBLIC_ORIGIN?: unknown; BETTER_AUTH_URL?: unknown; CONTROL_DB?: SqlDatabase };
function repository(control: SqlDatabase, holding?: SqlDatabase): InvitationDeliveryRepository {
  return {
    async waiting(holdingId) { return (await control.prepare("SELECT id,invitation_id FROM account_invitation_delivery_jobs WHERE holding_id=? AND state='awaiting_approval' ORDER BY created_at LIMIT 100").bind(holdingId).all<{ id: string; invitation_id: string }>()).results; },
    async review(invitationId) { if (!holding) throw new Error("INVITATION_APPROVAL_REPOSITORY_REQUIRED"); return (await holding.prepare("SELECT status FROM iam_reviews WHERE kind='invitation.authorize' AND payload->>'invitationId'=? ORDER BY created_at DESC LIMIT 1").bind(invitationId).first<{ status: string }>())?.status ?? null; },
    async promote(id, now) { await nativePrepare("w_65b69e372c88").bind(now, now, id).run(); },
    async reject(id, now) { await nativePrepare("w_b55dfa1352b0").bind(now, id).run(); },
    async recoverInterrupted(now) { await nativePrepare("w_22accdf2db68").bind(now, now, now).run(); },
    claim: (lease, now) => nativePrepare("w_44ab5b3afcae").bind(lease, now + 30000, now, now).first<InvitationDeliveryJob>(),
    invitation: id => control.prepare("SELECT email,name,status,expires_at FROM account_invitations WHERE id=?").bind(id).first<DeliverableInvitation>(),
    async cancel(id, lease, now) { await nativePrepare("w_256766a43bfa").bind(now, id, lease).run(); },
    async sent(id, lease, providerId, now) { await nativePrepare("w_11a8c78c2f43").bind(now, providerId, now, id, lease).run(); },
    async failed(id, lease, failure, now) { await nativePrepare("w_3392a4f9af95").bind(failure.state, failure.code, failure.nextAttemptAt, now, id, lease).run(); },
  };
}
export async function promoteAccountInvitationDeliveries(control: SqlDatabase, holding: SqlDatabase, holdingId: string, now = Date.now()) {
  const application = createInvitationDeliveryApplication({ repository: repository(control, holding), clock: { now: () => now }, identifiers: { next: () => crypto.randomUUID() }, activationLink: () => { throw new Error("DELIVERY_NOT_REQUESTED"); }, send: async () => { throw new Error("DELIVERY_NOT_REQUESTED"); } });
  await application.promote(holdingId);
}
export async function deliverAccountInvitation(control: SqlDatabase, env: DeliveryEnv, send: (env: DeliveryEnv, to: string, subject: string, url: string, key?: string) => Promise<string | null | void> = ((deliveryEnv,to,subject,url,key)=>sendBetaEmail(deliveryEnv as Parameters<typeof sendBetaEmail>[0],to,subject,url,key)), now = Date.now()) {
  return createInvitationDeliveryApplication({ repository: repository(control), clock: { now: () => now }, identifiers: { next: () => crypto.randomUUID() },
    activationLink(id) { const secret = String(env.BETTER_AUTH_SECRET ?? ""), origin = applicationOrigin("account", String(env.ERPX_PUBLIC_ORIGIN ?? env.BETTER_AUTH_URL ?? "")); if (!secret) throw new Error("INVITATION_SECRET_NOT_CONFIGURED"); if (!origin) throw new Error("PUBLIC_ORIGIN_NOT_CONFIGURED"); const token = createHmac("sha256", secret).update(`invitation:${id}`).digest("hex"); return new URL(`/activate?token=${encodeURIComponent(token)}`, origin).href; },
    send: (to, subject, url, key) => send(env, to, subject, url, key),
  }).deliver();
}
