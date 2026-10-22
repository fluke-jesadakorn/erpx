import { beforeEach, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({
  requestHeaders: new Headers(),
  redirect: vi.fn((path: string): never => { throw new Error(`redirect:${path}`); }),
  notFound: vi.fn((): never => { throw new Error("not-found"); }),
  forbidden: vi.fn((): never => { throw new Error("NEXT_HTTP_ERROR_FALLBACK;403"); }),
  unstable_rethrow: vi.fn((error: unknown) => {
    if (error && typeof error === "object" && "digest" in error && String(error.digest).startsWith("NEXT_")) throw error;
  }),
}));
vi.mock("next/navigation", () => mocks);
vi.mock("next/headers", () => ({ headers: async () => mocks.requestHeaders }));
vi.mock("@erpx/server-identity_access/public/authorization", () => ({ requestedHoldingScope: async (holdingSlug: string) => ({ kind: "department", holdingSlug, branchSlug: "bangkok", departmentSlug: "operations" }) }));
import { accessPage, pageError } from "@erpx/server-work_projection/public/page-error";
import { ApiError } from "@erpx/server-runtime/adapters/http";
beforeEach(() => {
  vi.clearAllMocks();
  mocks.requestHeaders = new Headers({ "x-erpx-destination": "people/organization" });
});
it("preserves the scoped destination when sign-in is required", async () => {
  await expect(pageError(new Error("UNAUTHENTICATED"), "acme")).rejects.toThrow("redirect:/login?");
  const url = new URL(mocks.redirect.mock.calls[0][0], "https://erpx.invalid");
  expect(url.searchParams.get("next")).toBe("/holdings/acme/branches/bangkok/departments/operations/people-organization/people/organization");
  expect(url.searchParams.get("reason")).toBe("sign_in");
});
it("retains the requested query when a session expires", async () => {
  mocks.requestHeaders.set("x-erpx-page-path", "/holdings/acme/finance?view=payments&status=draft");
  await expect(pageError(new ApiError(401, "SESSION_REVOKED", "Sign in again"), "acme")).rejects.toThrow("redirect:/login?");
  const url = new URL(mocks.redirect.mock.calls[0][0], "https://erpx.invalid");
  expect(url.searchParams.get("next")).toBe("/holdings/acme/finance?view=payments&status=draft");
});
it.each(["FORBIDDEN", "COMPANY_REQUIRED", "HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "INVALID_ORGANIZATION_LINEAGE"])("keeps %s at the requested URL with a forbidden boundary", async code => {
  await expect(pageError(new Error(code), "acme")).rejects.toThrow("NEXT_HTTP_ERROR_FALLBACK;403");
  expect(mocks.notFound).not.toHaveBeenCalled();
  expect(mocks.redirect).not.toHaveBeenCalled();
});
it.each(["WORK_ROLE_REQUIRED", "WORK_ROLE_UNAVAILABLE", "WORK_CONTEXT_CHANGED", "PLATFORM_CONTEXT_CHANGED"])("allows context recovery in place for %s", async code => {
  await expect(pageError(new ApiError(409, code, "Choose an available role"), "acme")).rejects.toThrow("NEXT_HTTP_ERROR_FALLBACK;403");
  expect(mocks.redirect).not.toHaveBeenCalled();
});
it.each([[409, "conflict"], [400, "invalid"], [422, "invalid"], [429, "rate_limited"]])("maps expected page status %s to %s", async (status, reason) => {
  await expect(pageError(new ApiError(Number(status), "DOMAIN_CODE", "Private details"), "acme")).rejects.toThrow(`reason=${reason}`);
  expect(mocks.redirect.mock.calls[0][0]).not.toContain("Private");
});
it.each([404, 410])("uses the not-found UI for missing/removed resources (%s)", async status => {
  await expect(pageError(new ApiError(status, "RECORD_NOT_FOUND", "Missing"), "acme")).rejects.toThrow("not-found");
});
it.each(["DOMAIN_CODE", "BETA_CAPABILITY_UNAVAILABLE"])("uses the forbidden UI for denied page capability %s", async code => {
  await expect(pageError(new ApiError(403, code, "Private details"), "acme")).rejects.toThrow("NEXT_HTTP_ERROR_FALLBACK;403");
  expect(mocks.redirect).not.toHaveBeenCalled();
});
it.each(["permission", "capability"] as const)("keeps explicit %s denials in the requested view", async reason => {
  await expect(accessPage(reason, "acme")).rejects.toThrow("NEXT_HTTP_ERROR_FALLBACK;403");
  expect(mocks.redirect).not.toHaveBeenCalled();
});
it("passes server failures and Next navigation interrupts through unchanged", async () => {
  for (const error of [new Error("database connection refused"), new ApiError(503, "SERVICE_UNAVAILABLE", "Private service details"), Object.assign(new Error("NEXT_REDIRECT"), { digest: "NEXT_REDIRECT;replace;/;307;" }), Object.assign(new Error("NEXT_HTTP_ERROR_FALLBACK;403"), { digest: "NEXT_HTTP_ERROR_FALLBACK;403" })]) {
    await expect(pageError(error, "acme")).rejects.toBe(error);
  }
  expect(mocks.notFound).not.toHaveBeenCalled(); expect(mocks.redirect).not.toHaveBeenCalled();
});
