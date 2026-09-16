import { beforeEach, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ redirect: vi.fn((path: string): never => { throw new Error(`redirect:${path}`); }), notFound: vi.fn((): never => { throw new Error("not-found"); }) }));
vi.mock("next/navigation", () => mocks);
vi.mock("next/headers", () => ({ headers: async () => new Headers({ "x-erpx-destination": "people/organization" }) }));
vi.mock("../src/server/authorization", () => ({ requestedHoldingScope: async (holdingSlug: string) => ({ kind: "department", holdingSlug, branchSlug: "bangkok", departmentSlug: "operations" }) }));
import { pageError } from "../src/server/page-error";
import { ApiError } from "../src/server/http";
beforeEach(() => vi.clearAllMocks());
it("preserves the scoped destination when sign-in is required", async () => {
  await expect(pageError(new Error("UNAUTHENTICATED"), "acme")).rejects.toThrow("redirect:/login?");
  const url = new URL(mocks.redirect.mock.calls[0][0], "https://erpx.invalid");
  expect(url.searchParams.get("next")).toBe("/holdings/acme/branches/bangkok/departments/operations/people/organization");
  expect(url.searchParams.get("reason")).toBe("sign_in");
});
it.each(["FORBIDDEN", "COMPANY_REQUIRED", "HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "INVALID_ORGANIZATION_LINEAGE"])("diagnoses %s rather than masking it with 404", async code => {
  await expect(pageError(new Error(code), "acme")).rejects.toThrow("redirect:/access?");
  expect(mocks.notFound).not.toHaveBeenCalled();
});
it.each([[403, "permission"], [409, "conflict"], [400, "invalid"], [422, "invalid"], [429, "rate_limited"]])("maps expected page status %s to %s", async (status, reason) => {
  await expect(pageError(new ApiError(Number(status), "DOMAIN_CODE", "Private details"), "acme")).rejects.toThrow(`reason=${reason}`);
  expect(mocks.redirect.mock.calls[0][0]).not.toContain("Private");
});
it.each([404, 410])("uses the not-found UI for missing/removed resources (%s)", async status => {
  await expect(pageError(new ApiError(status, "RECORD_NOT_FOUND", "Missing"), "acme")).rejects.toThrow("not-found");
});
it("distinguishes unavailable beta capabilities", async () => {
  await expect(pageError(new ApiError(403, "BETA_CAPABILITY_UNAVAILABLE", "Unavailable"), "acme")).rejects.toThrow("reason=capability");
});
it("passes server failures and Next navigation interrupts through unchanged", async () => {
  for (const error of [new Error("database connection refused"), new ApiError(503, "SERVICE_UNAVAILABLE", "Private service details"), Object.assign(new Error("NEXT_REDIRECT"), { digest: "NEXT_REDIRECT;replace;/;307;" })]) {
    await expect(pageError(error, "acme")).rejects.toBe(error);
  }
  expect(mocks.notFound).not.toHaveBeenCalled(); expect(mocks.redirect).not.toHaveBeenCalled();
});
