import { beforeEach, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  scopeKind: "holding" as "holding" | "branch",
  role: "holding_owner",
  platformRole: "platform_admin",
  calls: 0,
}));
vi.mock("../../../src/server/modules/identity_access/public/authorization", () => ({
  requestedHoldingScope: async () => ({ kind: state.scopeKind }),
  requireWorkContext: async () => ({
    role: state.role, scope: { kind: state.scopeKind }, holdingId: "h001",
  }),
  requireManageErpxContext: async () => ({ platform: { role: state.platformRole } }),
}));
vi.mock("../../../src/server/modules/operations/public/staff-policy", () => ({ requireCompanyAuthority: async () => undefined }));
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: {} }),
  runtimeEnv: async () => ({ CONTROL_DB: {
    pool: { query: async () => ({ rowCount: 1 }) },
  } }),
}));
vi.mock("../../../src/server/modules/billing/public/billing-forecast", () => ({
  readSeatForecastBaseline: async () => {
    state.calls++;
    return { forecastMonth: "2026-10", model: "seat_what_if",
      seats: { count: 282, observedAt: 1 }, storage: { originalBytes: 1, observedAt: 1 } };
  },
}));

import { GET as holdingGet } from "../../../src/app/api/v1/holdings/[holdingSlug]/billing/forecast/route";
import { GET as platformGet } from "../../../src/app/api/v1/platform/billing/forecast/route";

const holdingContext = { params: Promise.resolve({ holdingSlug: "acme" }) };
beforeEach(() => {
  state.scopeKind = "holding";
  state.role = "holding_owner";
  state.platformRole = "platform_admin";
  state.calls = 0;
});

it("serves only the Holding owner and never exposes a Branch forecast", async () => {
  const url = new Request("http://localhost:3100/api/v1/holdings/acme/billing/forecast");
  const owner = await holdingGet(url, holdingContext);
  expect(owner.status).toBe(200);
  expect(owner.headers.get("Cache-Control")).toBe("private, no-store");
  expect((await owner.json()).data.seats.count).toBe(282);
  state.scopeKind = "branch";
  expect((await holdingGet(url, holdingContext)).status).toBe(403);
  state.scopeKind = "holding";
  state.role = "operations_manager";
  expect((await holdingGet(url, holdingContext)).status).toBe(403);
  expect(state.calls).toBe(1);
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
