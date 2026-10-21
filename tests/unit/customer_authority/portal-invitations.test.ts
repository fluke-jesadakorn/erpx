import { expect, it } from "vitest";
import {
  customerPortalInvitationToken,
  customerPortalInvitationTokenHash,
  verifyCustomerPortalInvitationToken,
} from "../../../src/server/modules/customer_authority/adapters/portal-invitation-crypto";
import {
  createCustomerPortalInvitationApplication,
  type CustomerPortalInvitationCommand,
  type CustomerPortalInvitationRepository,
} from "../../../src/server/modules/customer_authority/application/portal-invitations";

const secret = "test-only-customer-portal-secret";
const tokenFacts = {
  invitationId: "invite-1",
  bindingId: "binding-1",
  holdingSlug: "northwind",
  generation: 1,
  expiresAt: 700,
  keyVersion: 1 as const,
};

function repository(operations: Partial<CustomerPortalInvitationRepository>): CustomerPortalInvitationRepository {
  return new Proxy(operations as CustomerPortalInvitationRepository, {
    get(target, name) {
      if (name === "then") return undefined;
      if (name in target) return Reflect.get(target, name);
      return () => { throw new Error(`unexpected operation: ${String(name)}`); };
    },
  });
}

function application(source: CustomerPortalInvitationRepository, options: { canManage?: boolean } = {}) {
  return createCustomerPortalInvitationApplication({
    actorId: "requester",
    holdingSlug: "northwind",
    canManage: () => options.canManage ?? true,
    clock: { now: () => 100 },
    identifiers: { next: () => "new-id" },
    token: facts => customerPortalInvitationToken(secret,facts),
    tokenHash: customerPortalInvitationTokenHash,
    repository: source,
    error: (_status, code, message) => new Error(`${code}:${message}`),
  });
}

const binding = (overrides: Record<string, unknown> = {}) => ({
  id: "binding-1",
  email: "customer@example.test",
  status: "approved",
  order_state: "confirmed",
  expires_at: 900,
  version: 4,
  created_by: "requester",
  ...overrides,
});

const issue = { action: "invite", expectedVersion: 4, reason: "Approved customer access" };

it("Customer Portal invitation signatures and hashes are deterministic for identical claims", () => {
  const first = customerPortalInvitationToken(secret, tokenFacts);
  const retry = customerPortalInvitationToken(secret, tokenFacts);

  expect(retry).toBe(first);
  expect(customerPortalInvitationTokenHash(retry)).toBe(customerPortalInvitationTokenHash(first));
  expect(verifyCustomerPortalInvitationToken(secret, first)).toEqual(tokenFacts);
});

it("Customer Portal invitation verification rejects modified signatures, claims, and malformed tokens", () => {
  const token = customerPortalInvitationToken(secret, tokenFacts);
  const [encoded, signature] = token.split(".");
  const changedClaim = Buffer.from(JSON.stringify({ ...tokenFacts, bindingId: "other-binding" })).toString("base64url");

  expect(verifyCustomerPortalInvitationToken(secret, `${encoded}.${signature.slice(0, -1)}${signature.endsWith("A") ? "B" : "A"}`)).toBeNull();
  expect(verifyCustomerPortalInvitationToken(secret, `${changedClaim}.${signature}`)).toBeNull();
  expect(verifyCustomerPortalInvitationToken(secret, "not-a-signed-token")).toBeNull();
});

it("issuing requires an approved binding that is still current", async () => {
  for (const row of [binding({ status: "pending" }), binding({ expires_at: 100 })]) {
    const source = repository({
      replay: async () => null,
      binding: async () => row,
    });

    await expect(application(source).issue("binding-1", issue, "command-1"))
      .rejects.toThrow("CUSTOMER_PORTAL_BINDING_UNAVAILABLE");
  }
});

it("issuing rejects an approved binding whose source order is no longer publishable", async () => {
  const source = repository({
    replay: async () => null,
    binding: async () => binding({ order_state: "cancelled" }),
  });

  await expect(application(source).issue("binding-1", issue, "command-cancelled-source"))
    .rejects.toThrow("CUSTOMER_PORTAL_BINDING_UNAVAILABLE");
});

it("issuing requires the original requester or an authorized sales manager", async () => {
  const source = repository({
    replay: async () => null,
    binding: async () => binding({ created_by: "another-user" }),
  });

  await expect(application(source, { canManage: false }).issue("binding-1", issue, "command-1"))
    .rejects.toThrow("FORBIDDEN");
});

it("does not issue invitations for a binding that already has an enrolled identity", async () => {
  const source = repository({
    replay: async () => null,
    binding: async () => binding({ identity_exists: true }),
  });

  await expect(application(source).issue("binding-1", issue, "command-identity-bound"))
    .rejects.toThrow("CUSTOMER_PORTAL_IDENTITY_ALREADY_BOUND");
});

it("a live invitation prevents a second issue for the same binding", async () => {
  const source = repository({
    replay: async () => null,
    binding: async () => binding(),
    active: async () => ({ id: "prior-invite", generation: 1, state: "queued", token_expires_at: 700 }),
  });

  await expect(application(source).issue("binding-1", issue, "command-2"))
    .rejects.toThrow("CUSTOMER_PORTAL_INVITATION_ACTIVE");
});

it("resend requires a live invitation and rejects a missing or expired invitation", async () => {
  const missing = repository({
    replay: async () => null,
    binding: async () => binding(),
    active: async () => null,
  });
  const expired = repository({
    replay: async () => null,
    binding: async () => binding(),
    active: async () => ({ id: "old-invite", generation: 1, state: "sent", token_expires_at: 100 }),
  });

  await expect(application(missing).issue("binding-1", { ...issue, action: "resend" }, "command-3"))
    .rejects.toThrow("CUSTOMER_PORTAL_INVITATION_NOT_ACTIVE");
  await expect(application(expired).issue("binding-1", { ...issue, action: "resend" }, "command-4"))
    .rejects.toThrow("CUSTOMER_PORTAL_INVITATION_NOT_ACTIVE");
});

it("resend advances the signed generation and persists only the token hash", async () => {
  let created: CustomerPortalInvitationCommand | undefined;
  let replaced: unknown;
  const source = repository({
    replay: async () => null,
    binding: async () => binding(),
    active: async () => ({ id: "prior-invite", generation: 1, state: "sent", token_expires_at: 700 }),
    nextGeneration: async () => 2,
    create: async (command, previous) => {
      created = command;
      replaced = previous;
      return { id: command.invitationId, generation: command.generation, state: "queued" };
    },
  });

  const result = await application(source).issue("binding-1", { ...issue, action: "resend" }, "command-5");
  expect(result).toMatchObject({ generation: 2, state: "queued" });
  expect(replaced).toMatchObject({ id: "prior-invite", generation: 1 });
  expect(created).toMatchObject({ generation: 2, tokenKeyVersion: 1, tokenExpiresAt: 900 });
  expect(created).not.toHaveProperty("token");

  const expectedToken = customerPortalInvitationToken(secret, {
    invitationId: "customer-portal-invitation-new-id",
    bindingId: "binding-1",
    holdingSlug: "northwind",
    generation: 2,
    expiresAt: 900,
    keyVersion: 1,
  });
  expect(created?.tokenHash).toBe(customerPortalInvitationTokenHash(expectedToken));
  expect(JSON.stringify(created)).not.toContain(expectedToken);
  expect(verifyCustomerPortalInvitationToken(secret, expectedToken)).toMatchObject({ generation: 2, bindingId: "binding-1" });
});
