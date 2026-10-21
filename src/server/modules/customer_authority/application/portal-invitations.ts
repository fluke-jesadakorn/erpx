import type { Clock, IdentifierSource } from "../../shared/application/ports";
import {
  customerPortalInvitationCommandSchema,
  customerPortalInvitationLifetimeMs,
  isCustomerPortalInvitableOrderState,
  type CustomerPortalInvitationTokenFacts,
} from "../domain/portal-invitation";

export type CustomerPortalInvitationBinding = {
  id: string;
  email: string;
  status: string;
  order_state: string;
  expires_at: number;
  version: number;
  created_by: string;
  identity_exists?: boolean;
};
export type CustomerPortalInvitationRow = {
  id: string;
  generation: number;
  state: string;
  token_expires_at: number;
};
export type CustomerPortalInvitationCommand = {
  key: string;
  operation: string;
  bindingId: string;
  expectedVersion: number;
  action: "invite" | "resend";
  reason: string;
  holdingSlug: string;
  invitationId: string;
  generation: number;
  tokenHash: string;
  tokenKeyVersion: 1;
  tokenExpiresAt: number;
  now: number;
};
export type CustomerPortalInvitationHistory = { invitations: Record<string, unknown>[]; invitationEvents: Record<string, unknown>[] };
export type CustomerPortalInvitationRepository = {
  binding(id: string): Promise<CustomerPortalInvitationBinding | null>;
  active(id: string, now: number): Promise<CustomerPortalInvitationRow | null>;
  nextGeneration(id: string): Promise<number>;
  replay(input: { key: string; operation: string; bindingId: string; actorId: string; input: unknown }): Promise<Record<string, unknown> | null>;
  create(input: CustomerPortalInvitationCommand, previous: CustomerPortalInvitationRow | null): Promise<Record<string, unknown>>;
  history(id: string): Promise<CustomerPortalInvitationHistory>;
};

export function createCustomerPortalInvitationApplication(ports: {
  actorId: string;
  holdingSlug: string;
  canManage(): boolean;
  clock: Clock;
  identifiers: IdentifierSource;
  token(facts: CustomerPortalInvitationTokenFacts): string;
  tokenHash(token: string): string;
  repository: CustomerPortalInvitationRepository;
  error(status: number, code: string, message: string): Error;
}) {
  const fail = (status: number, code: string, message: string): never => { throw ports.error(status, code, message); };
  return {
    async issue(bindingId: string, raw: unknown, key: string) {
      const input = customerPortalInvitationCommandSchema.parse(raw), operation = `customer.access.${input.action}`;
      const prior = await ports.repository.replay({ key, operation, bindingId, actorId: ports.actorId, input });
      if (prior) return prior;
      const binding = await ports.repository.binding(bindingId);
      if (!binding) return fail(404, "CUSTOMER_ACCESS_NOT_FOUND", "Customer access request not found in this scope.");
      if (binding.version !== input.expectedVersion) return fail(409, "CONFLICT", "Customer access changed before the invitation was issued.");
      if (!ports.canManage()) return fail(403, "FORBIDDEN", "Only the original requester or an authorized sales manager may invite this customer.");
      if (binding.identity_exists) return fail(409, "CUSTOMER_PORTAL_IDENTITY_ALREADY_BOUND", "This approved binding already belongs to a verified Portal account.");
      const now = ports.clock.now();
      if (binding.status !== "approved" || Number(binding.expires_at) <= now || !isCustomerPortalInvitableOrderState(binding.order_state))
        return fail(409, "CUSTOMER_PORTAL_BINDING_UNAVAILABLE", "Only a current approved customer binding can be invited.");

      const active = await ports.repository.active(bindingId, now);
      if (active?.state === "accepted")
        return fail(409, "CUSTOMER_PORTAL_IDENTITY_ALREADY_BOUND", "This approved binding already belongs to a verified Portal account.");
      const activeExpired = active !== null && Number(active.token_expires_at) <= now;
      if (input.action === "invite" && active && !activeExpired)
        return fail(409, "CUSTOMER_PORTAL_INVITATION_ACTIVE", "A current invitation already exists. Resend it to replace the previous link.");
      if (input.action === "resend" && (!active || activeExpired))
        return fail(409, "CUSTOMER_PORTAL_INVITATION_NOT_ACTIVE", "There is no current invitation to resend. Issue a new invitation instead.");

      const tokenExpiresAt = Math.min(Number(binding.expires_at), now + customerPortalInvitationLifetimeMs);
      if (tokenExpiresAt <= now) return fail(409, "CUSTOMER_PORTAL_BINDING_UNAVAILABLE", "The approved customer binding has expired.");
      const invitationId = `customer-portal-invitation-${ports.identifiers.next()}`;
      const facts: CustomerPortalInvitationTokenFacts = {
        invitationId,
        bindingId,
        holdingSlug: ports.holdingSlug,
        generation: await ports.repository.nextGeneration(bindingId),
        expiresAt: tokenExpiresAt,
        keyVersion: 1,
      };
      const token = ports.token(facts);
      return ports.repository.create({
        key, operation, bindingId, expectedVersion: input.expectedVersion, action: input.action, reason: input.reason, holdingSlug: ports.holdingSlug,
        invitationId, generation: facts.generation, tokenHash: ports.tokenHash(token), tokenKeyVersion: 1,
        tokenExpiresAt, now,
      }, active);
    },
    history(bindingId: string) { return ports.repository.history(bindingId); },
  };
}
