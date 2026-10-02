import { beforeEach, expect, it, vi } from "vitest";
import { legalProductionScope, productionScope } from "../src/data/production-scope";

const state = vi.hoisted(() => ({
  scopeVersion: "legal-th-operations-v1",
  acceptance: vi.fn(),
  review: vi.fn(),
  revoke: vi.fn(),
  write: { name: "holding-write" },
}));
vi.mock("../src/server/authorization", () => ({
  requireWorkContext: async () => ({ holdingId: "h001", companyId: "company-a", userId: "reviewer" }),
}));
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: state.write }),
}));
vi.mock("../src/server/public-origin", () => ({
  requestPublicOrigin: () => "https://dev-erpx.example.test",
}));
vi.mock("../src/server/release-acceptance", () => ({
  releaseAcceptance: state.acceptance,
  submitReleaseAcceptance: vi.fn(),
  reviewReleaseAcceptance: state.review,
  revokeReleaseAcceptance: state.revoke,
  acceptedReleasePredicate: () => "FALSE",
}));

import { POST } from "../src/app/api/v1/holdings/[holdingSlug]/legal/[[...path]]/route";

beforeEach(() => {
  state.scopeVersion = legalProductionScope.version;
  state.acceptance.mockReset().mockImplementation(async () => ({
    release: { id: "release-1", scopeVersion: state.scopeVersion },
  }));
  state.review.mockReset().mockResolvedValue({ id: "release-1", version: 2 });
  state.revoke.mockReset().mockResolvedValue({ id: "release-1", version: 2 });
});

function post(action: "review" | "revoke") {
  return POST(new Request(`https://dev-erpx.example.test/api/v1/holdings/acme/legal/release-acceptance/release-1/${action}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Origin: "https://dev-erpx.example.test", "Idempotency-Key": "release-command-1" },
    body: JSON.stringify({ expectedVersion: 1, reason: "Independent review of the submitted evidence" }),
  }), { params: Promise.resolve({ holdingSlug: "acme", path: ["release-acceptance", "release-1", action] }) });
}

it.each(["review", "revoke"] as const)("rejects a hotel release ID through Legal %s before invoking a mutation", async action => {
  state.scopeVersion = productionScope.version;
  const response = await post(action);
  expect(response.status).toBe(404);
  expect(await response.json()).toMatchObject({ error: { code: "RELEASE_NOT_FOUND" } });
  expect(state.review).not.toHaveBeenCalled();
  expect(state.revoke).not.toHaveBeenCalled();
});

it.each(["review", "revoke"] as const)("allows Legal %s only after checking the current Company release", async action => {
  const response = await post(action);
  expect(response.status).toBe(200);
  expect(state.acceptance).toHaveBeenCalledWith(state.write, expect.objectContaining({ companyId: "company-a" }), "release-1", legalProductionScope.version);
  expect(action === "review" ? state.review : state.revoke).toHaveBeenCalledOnce();
  expect(response.headers.get("cache-control")).toBe("private, no-store");
});
