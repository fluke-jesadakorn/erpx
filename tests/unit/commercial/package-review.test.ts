import { expect, it } from "vitest";
import { packageReviewSigner } from "@/server/modules/commercial/adapters/package-review";
import type { PackageGrantBatch } from "@/server/modules/commercial/public/package-grants";
const actor = { userId: "fixture-provider", sessionId: "fixture-admin-session", assignmentId: "fixture-platform-assignment", role: "platform_admin", roleRevision: 3 };
const input: PackageGrantBatch = { companyId: "fixture-company", expectedRevision: "a".repeat(64), idempotencyKey: "fixture-idempotency-key", changes: [{ expectedVersion: 0,
  grant: { companyId: "fixture-company", packageId: "work", packageVersion: 1, version: 1, state: "active", startsAt: 1000, endsAt: null, reason: "Fictional reviewed reason", evidence: "test:review" } }] };
it("binds a signed review to actor, session, assignment, role revision, Holding and the exact canonical request", () => {
  const signer = packageReviewSigner("fixture-only-secret-32-characters-minimum", () => 1000);
  const review = signer.issue(actor, "h001", input, "b".repeat(64));
  expect(signer.read(review.token, actor, "h001", input).authorityRevision).toBe("b".repeat(64));
  for (const changed of [{ ...actor, userId: "other" }, { ...actor, sessionId: "other" }, { ...actor, assignmentId: "other" }, { ...actor, roleRevision: 4 }])
    expect(() => signer.read(review.token, changed, "h001", input)).toThrow("Preview this Company");
  for (const changed of [{ ...input, companyId: "other" }, { ...input, idempotencyKey: "another-reviewed-key" },
    { ...input, changes: [{ ...input.changes[0], grant: { ...input.changes[0].grant, reason: "Altered reason" } }] }])
    expect(() => signer.read(review.token, actor, "h001", changed)).toThrow("Preview this Company");
  expect(() => signer.read(review.token, actor, "h002", input)).toThrow("Preview this Company");
  expect(() => signer.read(review.token.replace(/^./, review.token[0] === "e" ? "f" : "e"), actor, "h001", input)).toThrow("Preview this Company");
});
it("rejects exact-boundary expiry for new writes while retaining signature verification for committed replay", () => {
  let instant = 1000;
  const signer = packageReviewSigner("fixture-only-secret-32-characters-minimum", () => instant), review = signer.issue(actor, "h001", input, "b".repeat(64));
  instant = review.expiresAt - 1; signer.assertFresh(signer.read(review.token, actor, "h001", input));
  instant++; const claims = signer.read(review.token, actor, "h001", input);
  expect(() => signer.assertFresh(claims)).toThrow("This preview expired");
});
