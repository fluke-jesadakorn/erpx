import { beforeEach, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  scope: { kind: "holding", holdingSlug: "acme" } as { kind: "holding" | "branch"; holdingSlug: string; branchSlug?: string },
  role: "holding_owner",
  platformRole: "platform_admin",
  selectedBranch: "bangkok",
  grantBranch: "bangkok",
  readBranch: null as string | null,
}));

vi.mock("../../../src/server/modules/identity_access/public/authorization", () => ({
  requestedHoldingScope: async () => state.scope,
  requireWorkContext: async () => ({
    role: state.role, assignmentScope: state.role === "operations_manager" ? "branch" : "holding",
    assignmentId: "selected", availableAssignments: [{ id: "selected", scopeNodeId: state.grantBranch }],
    scope: state.scope.kind === "branch"
      ? { kind: "branch", branchId: state.selectedBranch }
      : { kind: "holding" },
    holdingId: "h001", holdingSlug: "acme", holdingName: "Acme Group",
    iam: { permissions: ["billing.usage.read"] },
  }),
  requireManageErpxContext: async () => ({ platform: { role: state.platformRole } }),
}));
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  activeHoldingBindings: async () => [{ id: "h001", slug: "acme", displayName: "Acme Group" }],
  runtimeEnv: async () => ({ CONTROL_DB: {} }),
}));
vi.mock("../../../src/server/modules/commercial/public/usage", () => ({
  usageMonth: (month: string | null) => ({ month: month ?? "2026-09" }),
  readHoldingUsage: async (_binding: unknown, _month: string, branchId: string | null) => {
    state.readBranch = branchId;
    const totals = { ai: { requests: 1, inputTokens: 5, outputTokens: 3 }, storage: { bytes: 10, fileCount: 1, averageDailyBytes: 5 } };
    return {
      holding: { holdingId: "h001", holdingSlug: "acme", holdingName: "Acme Group",
        companies: [{ companyId: "c001", companyName: "Company", branches: [{ branchId: "bangkok", branchName: "Bangkok", totals }], unassigned: totals, totals }],
        totals, sandboxStoragePreview: { status: "estimated", amountSatang: 50 },
        meterCoverage: { ai: "partial", storage: "partial", seat:"unavailable", unreconciledOriginals: 1 },
        seatUsage:{monthly:{status:"unavailable",distinctUsers:null,activeSeatMilliseconds:null},
          activeNow:{status:"measured",count:7,asOf:1000}} },
      items: [{ id: "meter", holdingId: "h001", companyId: "c001", companyName: "Company",
        branchId: "bangkok", branchName: "Bangkok", type: "ai", quantity: 8, unit: "tokens",
        period: "2026-09", source: "AI · Failed", requestStatus: "failed",
        pricingStatus: "unpriced", estimatedSatang: null, fundingSource: "erpx",
        averageDailyBytes: 5, activeAtPeriodEnd: true }],
    };
  },
}));
vi.mock("../../../src/server/modules/commercial/public/usage-projection", () => ({
  readPlatformUsageProjection: async () => ({ holdings: [], items: [], unavailable: [] }),
}));

import { GET as holdingGet } from "../../../src/app/api/v1/holdings/[holdingSlug]/usage/route";
import { GET as platformGet } from "../../../src/app/api/v1/platform/billing/usage/route";

const context = { params: Promise.resolve({ holdingSlug: "acme" }) };
const request = () => new Request("http://localhost:3100/api/v1/holdings/acme/usage?month=2026-09");

beforeEach(() => {
  state.scope = { kind: "holding", holdingSlug: "acme" };
  state.role = "holding_owner";
  state.platformRole = "platform_admin";
  state.selectedBranch = "bangkok";
  state.grantBranch = "bangkok";
  state.readBranch = null;
});

it("lets a Holding owner read all measured usage", async () => {
  const response = await holdingGet(request(), context);
  expect(response.status).toBe(200);
  expect(state.readBranch).toBeNull();
  const report = (await response.json()).data;
  expect(report.scope).toBe("holding");
  expect(report.holdings[0].sandboxStoragePreview.amountSatang).toBe(50);
  expect(report.holdings[0].seatUsage.activeNow.count).toBe(7);
});

it("limits a branch manager to the selected branch and strips billing fields", async () => {
  state.scope = { kind: "branch", holdingSlug: "acme", branchSlug: "bangkok" };
  state.role = "operations_manager";
  const response = await holdingGet(request(), context);
  expect(response.status).toBe(200);
  expect(state.readBranch).toBe("bangkok");
  const report = (await response.json()).data;
  expect(report.scope).toBe("branch");
  expect(report.holdings[0].sandboxStoragePreview).toBeUndefined();
  expect(report.holdings[0].totals.storage.averageDailyBytes).toBeUndefined();
  expect(report.holdings[0].seatUsage).toBeUndefined();
  expect(report.holdings[0].meterCoverage.seat).toBeUndefined();
  expect(report.items[0].pricingStatus).toBeUndefined();
  expect(report.items[0].fundingSource).toBeUndefined();
  expect(report.items[0].averageDailyBytes).toBeUndefined();
  expect(report.items[0].requestStatus).toBe("failed");
});

it("denies a manager using Holding scope or a different Branch assignment", async () => {
  state.role = "operations_manager";
  expect((await holdingGet(request(), context)).status).toBe(403);
  state.scope = { kind: "branch", holdingSlug: "acme", branchSlug: "bangkok" };
  state.grantBranch = "chiang-mai";
  expect((await holdingGet(request(), context)).status).toBe(403);
  expect(state.readBranch).toBeNull();
});

it("allows Platform Admin and Auditor, while denying another Platform role", async () => {
  const platformRequest = new Request("http://localhost:3100/api/v1/platform/billing/usage?month=2026-09");
  for (const role of ["platform_admin", "platform_auditor"]) {
    state.platformRole = role;
    expect((await platformGet(platformRequest)).status).toBe(200);
  }
  state.platformRole = "platform_support";
  expect((await platformGet(platformRequest)).status).toBe(403);
});
