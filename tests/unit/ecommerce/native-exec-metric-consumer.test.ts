import { describe, expect, test, vi } from "vitest";

const m = vi.hoisted(() => ({ native: vi.fn(), sources: vi.fn() }));
vi.mock("server-only", () => ({}));
vi.mock("../../../packages/server-ecommerce/src/adapters/native-exec-metric/client", () => ({
  nativeExecutiveMetric: (...args: unknown[]) => m.native(...args),
}));
vi.mock("@erpx/server-runtime/public/executive/metric-sources", () => ({
  readMetricSources: (...args: unknown[]) => m.sources(...args),
}));

import { specs } from "@erpx/server-runtime/public/executive/metric-projection-contract";
import { readExecutiveMetricProjection } from "../../../packages/server-ecommerce/src/adapters/executive-metric-projections";

const SQL =
  "SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) AND c.status='open' GROUP BY 1 ORDER BY 1";
const RAW = [{ label: "open", value: "3" }];
const VALUES = ["co-1", null, null, null, null] as const;
const ACTOR = {
  companyId: "co-1",
  branchId: null,
  departmentId: null,
  holdingId: "h1",
  holdingSlug: "acme",
} as any;
const FILTERS = { from: "2026-01-01", to: "2026-01-31", currency: "THB" } as any;
const ID = "open-ecommerce-carts" as any;

describe("Ecommerce executive metric consumer delegates to the native port", () => {
  test("forwards scope, metric and filters; issues no owner SQL before the source drill-down", async () => {
    m.native.mockResolvedValue({ raw: RAW, sql: SQL, values: [...VALUES] });
    m.sources.mockResolvedValue({ row: {}, rows: [], page: 1, hasNext: false });
    const db = { prepare: vi.fn() } as any;
    const snapshot = await readExecutiveMetricProjection(db, ACTOR, ID, FILTERS, "status" as any);

    expect(m.native).toHaveBeenCalledWith(
      {
        companyId: "co-1",
        branchId: null,
        departmentId: null,
        metric: ID,
        breakdown: "status",
        from: "2026-01-01",
        to: "2026-01-31",
      },
      ACTOR,
    );
    expect(db.prepare).not.toHaveBeenCalled();
    expect(snapshot.raw).toBe(RAW);
    expect(snapshot.referenceAtOverride).toBe(null);
    expect(await snapshot.watermarks()).toEqual([]);

    const group = { label: "open", page: 1 } as any;
    await snapshot.source(group, "3");
    expect(m.sources).toHaveBeenCalledWith(
      db,
      ID,
      SQL,
      [...VALUES],
      group,
      RAW,
      `${specs["open-ecommerce-carts"].dateBasis} · 2026-01-01 to 2026-01-31 · THB`,
      "3",
    );
  });
});
