import { beforeEach, expect, it, vi } from "vitest";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expandNativeApiPaths, loadNativeApiPaths } from "../../../scripts/local/native-api-paths";
import { nativeApiPath } from "../../../scripts/local/native-api-proxy";

const state = vi.hoisted(() => ({
  scopeKind: "holding" as "holding" | "branch",
  role: "holding_owner",
  platformRole: "platform_admin",
  calls: 0,
}));
vi.mock("@erpx/identity-access-server/public/authorization", () => ({
  requestedHoldingScope: async () => ({ kind: state.scopeKind }),
  requireWorkContext: async () => ({
    role: state.role, scope: { kind: state.scopeKind }, holdingId: "h001",
  }),
  requireManageErpxContext: async () => ({ platform: { role: state.platformRole } }),
}));
vi.mock("@erpx/operations-server/public/staff-policy", () => ({ requireCompanyAuthority: async () => undefined }));
vi.mock("next/headers", () => ({ headers: async () => new Headers({ host: "acme.jesadakorn.com" }) }));
vi.mock("@erpx/platform-runtime-server/public/app-native-sources", () => ({
  customerHoldingBindingExists: async () => true,
}));
vi.mock("../../../platform/runtime/server/src/public/runtime", () => ({
  holdingResources: async () => ({ write: {} }),
  runtimeEnv: async () => ({ CONTROL_DB: {} }),
}));
vi.mock("@erpx/billing-server/public/native-billing", () => ({
  nativeBillingConfiguration: () => ({}),
  nativeBillingFailure: () => undefined,
  createNativeBillingClient: () => ({
    holding: (id: string) => (id === "h001" ? { holdingSlug: "acme" } : undefined),
    forecast: async () => {
      state.calls++;
      return { forecastMonth: "2026-10", model: "seat_what_if",
        seats: { count: 282, observedAt: 1 }, storage: { originalBytes: 1, observedAt: 1 } };
    },
  }),
}));

import { GET as platformGet } from "@erpx/product-admin-ui/routes/api/v1/platform/billing/forecast/route";

beforeEach(() => {
  vi.stubEnv("ERPX_NATIVE_BILLING_CONFIG", "{}");
  state.scopeKind = "holding";
  state.role = "holding_owner";
  state.platformRole = "platform_admin";
  state.calls = 0;
});

it("keeps the retired Holding forecast Next route gone and its native gateway path live", () => {
  for (const file of [
    "products/admin/ui/src/routes/api/v1/holdings/[holdingSlug]/billing/forecast/route.ts",
    "apps/admin/src/app/api/v1/holdings/[holdingSlug]/billing/forecast/route.ts",
    "apps/compat/src/app/api/v1/holdings/[holdingSlug]/billing/forecast/route.ts",
  ]) expect(existsSync(new URL(`../../../${file}`, import.meta.url)), file).toBe(false);
  const config = loadNativeApiPaths(fileURLToPath(new URL("../../../scripts/local/native-api-paths.json", import.meta.url)));
  const { paths } = expandNativeApiPaths(config, [{ hostname: "acme.jesadakorn.com", holdingSlug: "acme" }]);
  const environment = { NODE_ENV: "test" as const, ERPX_DEV_NATIVE_PATHS: paths };
  expect(nativeApiPath(environment, "/api/v1/holdings/acme/billing/forecast", "acme.jesadakorn.com", "GET")).toBe(true);
  expect(nativeApiPath(environment, "/api/v1/holdings/acme/billing/target", "acme.jesadakorn.com", "GET")).toBe(false);
});

it("allows Platform Admin and Auditor, but not unrelated Platform roles", async () => {
  const url = new Request("http://localhost:3100/api/v1/platform/billing/forecast?holdingId=h001");
  expect((await platformGet(url)).status).toBe(200);
  state.platformRole = "platform_auditor";
  expect((await platformGet(url)).status).toBe(200);
  state.platformRole = "platform_support";
  expect((await platformGet(url)).status).toBe(403);
  expect(state.calls).toBe(2);
  state.platformRole = "platform_admin";
  expect((await platformGet(new Request("http://localhost:3100/api/v1/platform/billing/forecast?holdingId=h003"))).status).toBe(422);
});
