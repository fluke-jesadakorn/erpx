export type InvitationDeliveryJob = { id: string; invitation_id: string; attempts: number; lease_token: string };
export type DeliverableInvitation = { email: string; name: string; status: string; expires_at: number };

export function invitationCanBeDelivered(invitation: DeliverableInvitation | null, now: number): invitation is DeliverableInvitation {
  return !!invitation && invitation.status === "pending" && invitation.expires_at > now;
}
export function invitationDeliveryFailure(error: unknown, attempts: number, now: number) {
  const code = error instanceof Error && ["EMAIL_NOT_CONFIGURED", "PUBLIC_ORIGIN_NOT_CONFIGURED", "INVITATION_SECRET_NOT_CONFIGURED"].includes(error.message) ? error.message : "EMAIL_DELIVERY_FAILED";
  return { state: attempts >= 6 ? "failed" as const : "queued" as const, code, nextAttemptAt: now + Math.min(3600000, 30000 * 2 ** attempts) };
}
