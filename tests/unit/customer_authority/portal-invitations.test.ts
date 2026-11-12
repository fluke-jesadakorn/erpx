import { expect, it } from "vitest";
import {
  customerPortalInvitationToken,
  customerPortalInvitationTokenHash,
  verifyCustomerPortalInvitationToken,
} from "@erpx/server-customer_authority/adapters/portal-invitation-crypto";

const secret = "test-only-customer-portal-secret";
const tokenFacts = {
  invitationId: "invite-1",
  bindingId: "binding-1",
  holdingSlug: "northwind",
  generation: 1,
  expiresAt: 700,
  keyVersion: 1 as const,
};

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
