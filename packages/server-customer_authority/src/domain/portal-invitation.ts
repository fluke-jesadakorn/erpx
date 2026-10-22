import { z } from "zod";

export const customerPortalInvitationLifetimeMs = 7 * 86400000;
export const customerPortalInvitationAudience = "erpx-customer-portal-invitation-v1";
export const customerPortalInvitableOrderStates = ["confirmed", "converted", "partially_fulfilled", "fulfilled"] as const;
export function isCustomerPortalInvitableOrderState(state: string): boolean {
  return (customerPortalInvitableOrderStates as readonly string[]).includes(state);
}

export const customerPortalInvitationCommandSchema = z.object({
  action: z.enum(["invite", "resend"]),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});

export const customerPortalInvitationClaimsSchema = z.object({
  invitationId: z.string().min(1).max(200),
  bindingId: z.string().min(1).max(200),
  holdingSlug: z.string().min(1).max(100),
  generation: z.number().int().positive(),
  expiresAt: z.number().int().positive(),
  keyVersion: z.literal(1),
});

export type CustomerPortalInvitationClaims = z.infer<typeof customerPortalInvitationClaimsSchema>;
export type CustomerPortalInvitationTokenFacts = CustomerPortalInvitationClaims;
