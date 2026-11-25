import { afterEach, describe, expect, test, vi } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("server-only", () => ({}));

import {
  LANE17_RD_LANDED_COST_MOVED_STOCK,
  LANE17_RD_LANDED_COST_SCOPED,
  LANE17_RD_LANDED_COST_WORK_ROWS,
  LANE17_RD_LANDED_COSTS_FOR_ORDER,
  LANE17_RD_REORDER_POLICIES,
  LANE17_RD_SERVICE_SPARES,
  readLane17Rows,
  readLane17ReplenishmentRows,
  nativeInventoryLane17Configured,
} from "../../../packages/server-inventory/src/adapters/native-lane-17/client";

const ROOT = new URL("../../../", import.meta.url).pathname.replace(/\/$/, "");
const CONFIG_KEY = "ERPX_NATIVE_INVENTORY_LANE17_READ_CONFIG";
const HOLDINGS = [
  { origin: "https://acme.jesadakorn.com", host: "acme.jesadakorn.com", holdingId: "h001", holdingSlug: "acme" },
  { origin: "https://northstar.jesadakorn.com", host: "northstar.jesadakorn.com", holdingId: "h002", holdingSlug: "northstar" },
  { origin: "https://erpx-internal.jesadakorn.com", host: "erpx-internal.jesadakorn.com", holdingId: "h003", holdingSlug: "erpx" },
];
const ACTOR = { holdingId: "h001", holdingSlug: "acme", companyId: "company-1" } as const;
const env = (holdings: unknown = HOLDINGS, extra: Record<string, string | undefined> = {}) => ({
  [CONFIG_KEY]: JSON.stringify({
    privateBaseUrl: "http://127.0.0.1:3198",
    timeoutMs: 10000,
    holdings,
  }),
  ...extra,
});
const scope = [LANE17_RD_LANDED_COST_SCOPED, ["cost-1", "company-1", null, null], ACTOR] as const;

const ok = (rows: unknown[] = [{ id: "cost-1", status: "posted", amount_minor: 12500 }]) =>
  new Response(JSON.stringify({ rows }), { status: 200, headers: { "content-type": "application/json" } });

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("native Inventory lane 17 transport", () => {
  test("emits only bounded identities, exact WorkContext target and Company header", async () => {
    const fetchMock = vi.fn(async () => ok());
    vi.stubGlobal("fetch", fetchMock);
    const rows = await readLane17Rows(...scope, env());
    expect(rows).toEqual([{ id: "cost-1", status: "posted", amount_minor: 12500 }]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.toString()).toBe(
      "http://127.0.0.1:3198/api/v1/holdings/acme/inventory/native-lane-17/read",
    );
    expect(init.method).toBe("POST");
    expect((init.headers as Record<string, string>)["host"]).toBe("acme.jesadakorn.com");
    expect((init.headers as Record<string, string>)["x-erpx-context-company"]).toBe("company-1");
    expect(JSON.parse(String(init.body))).toEqual({ kind: scope[0], values: scope[1] });
    expect(String(init.body)).not.toMatch(/SELECT|FROM |WHERE /i);
  });

  test("selects the exact registered Holding by WorkContext, never the first holding", async () => {
    const fetchMock = vi.fn(async () => ok([]));
    vi.stubGlobal("fetch", fetchMock);
    await readLane17Rows(
      LANE17_RD_LANDED_COST_SCOPED,
      ["cost-2", "company-2", null, null],
      { holdingId: "h002", holdingSlug: "northstar", companyId: "company-2" },
      env(),
    );
    await readLane17Rows(
      LANE17_RD_LANDED_COST_SCOPED,
      ["cost-3", "company-3", null, null],
      { holdingId: "h003", holdingSlug: "erpx", companyId: "company-3" },
      env(),
    );
    const urls = fetchMock.mock.calls.map((call) => (call as unknown as [URL])[0].toString());
    expect(urls).toContain("http://127.0.0.1:3198/api/v1/holdings/northstar/inventory/native-lane-17/read");
    expect(urls).toContain("http://127.0.0.1:3198/api/v1/holdings/erpx/inventory/native-lane-17/read");
    for (const actor of [
      { holdingId: "h999", holdingSlug: "acme", companyId: "company-1" },
      { holdingId: "h001", holdingSlug: "northstar", companyId: "company-1" },
    ]) {
      await expect(
        readLane17Rows(scope[0], scope[1], actor, env()),
      ).rejects.toMatchObject({ status: 409, code: "WORK_CONTEXT_CHANGED" });
    }
  });

  test("a configured holding companyId pins the caller's Company", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => ok()));
    const configured = [{ ...HOLDINGS[0], companyId: "company-1" }];
    await expect(readLane17Rows(...scope, env(configured))).resolves.toEqual([
      { id: "cost-1", status: "posted", amount_minor: 12500 },
    ]);
    await expect(
      readLane17Rows(
        LANE17_RD_LANDED_COST_SCOPED,
        ["cost-1", "company-9", null, null],
        { ...ACTOR, companyId: "company-9" },
        env(configured),
      ),
    ).rejects.toMatchObject({ status: 409, code: "WORK_CONTEXT_CHANGED" });
  });

  test("rejects unknown kinds, wrong value counts, non-scalars and scope mismatch before fetch", async () => {
    const fetchMock = vi.fn(async () => ok());
    vi.stubGlobal("fetch", fetchMock);
    for (const args of [
      ["landed_cost_write", ["cost-1"]],
      [LANE17_RD_LANDED_COST_SCOPED, ["cost-1"]],
      [LANE17_RD_LANDED_COST_SCOPED, [{ id: "cost-1" }, "company-1", null, null]],
      [LANE17_RD_LANDED_COST_SCOPED, ["cost-1", "company-2", null, null]],
    ] as const) {
      await expect(readLane17Rows(args[0], args[1], ACTOR, env())).rejects.toMatchObject({ status: 503 });
    }
    expect(fetchMock).not.toHaveBeenCalled();
  });

  test("fails closed with 503 on missing config, out-of-range holdings, network error, 5xx and malformed rows", async () => {
    await expect(readLane17Rows(...scope, {})).rejects.toMatchObject({
      status: 503,
      code: "NATIVE_INVENTORY_LANE17_CONFIG_REQUIRED",
    });
    for (const holdings of [[], new Array(65).fill(HOLDINGS[0])]) {
      await expect(readLane17Rows(...scope, env(holdings))).rejects.toMatchObject({
        status: 503,
        code: "NATIVE_INVENTORY_LANE17_CONFIG_INVALID",
      });
    }
    const fetchMock = vi.fn(async () => ok());
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("fetch", vi.fn(async () => { throw new Error("offline"); }));
    await expect(readLane17Rows(...scope, env())).rejects.toMatchObject({ status: 503 });
    vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 503 })));
    await expect(readLane17Rows(...scope, env())).rejects.toMatchObject({ status: 503 });
    for (const rows of [[{ nested: { x: 1 } }], "not-an-array", new Array(501).fill({ id: "x" })]) {
      vi.stubGlobal("fetch", vi.fn(async () => ok(rows as unknown[])));
      await expect(readLane17Rows(...scope, env())).rejects.toMatchObject({
        status: 503,
        code: "NATIVE_INVENTORY_LANE17_RESPONSE_INVALID",
      });
    }
    expect(fetchMock).not.toHaveBeenCalled();
  });

  test("preserves owner 4xx error semantics", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(JSON.stringify({ error: { code: "INVENTORY_LANE17_KIND_INVALID", message: "Rejected." } }), { status: 422 }),
      ),
    );
    await expect(readLane17Rows(...scope, env())).rejects.toMatchObject({
      status: 422,
      code: "INVENTORY_LANE17_KIND_INVALID",
      message: "Rejected.",
    });
  });

  test("exposes the configuration switch without starting a fallback reader", () => {
    expect(nativeInventoryLane17Configured(env())).toBe(true);
    expect(nativeInventoryLane17Configured({})).toBe(false);
  });

  test("order-scoped reads carry no SQL and an owner-side company witness", async () => {
    const fetchMock = vi.fn(async () => ok([]));
    vi.stubGlobal("fetch", fetchMock);
    await readLane17Rows(LANE17_RD_LANDED_COST_MOVED_STOCK, ["fulfillment-1", "warehouse-1", "2026-01-01T00:00:00Z"], ACTOR, env());
    await readLane17Rows(LANE17_RD_LANDED_COST_WORK_ROWS, ["company-1", null], ACTOR, env());
    expect(fetchMock).toHaveBeenCalledTimes(2);
    for (const call of fetchMock.mock.calls) {
      expect(String((call as unknown as [URL, RequestInit])[1].body)).not.toMatch(/SELECT|FROM |WHERE /i);
    }
    await expect(
      readLane17Rows(LANE17_RD_LANDED_COSTS_FOR_ORDER, ["order-1", "company-2", null, null], ACTOR, env()),
    ).rejects.toMatchObject({ status: 503 });
  });
});

describe("native Inventory lane 17 replenishment reads", () => {
  const policies = [LANE17_RD_REORDER_POLICIES, ["company-1", null, null], ACTOR] as const;
  const spares = [LANE17_RD_SERVICE_SPARES, ["company-1", "branch-1"], ACTOR] as const;

  test("reorder policies and service spares use the WorkContext-selected target", async () => {
    const fetchMock = vi.fn(async () => ok([{ id: "policy-1" }]));
    vi.stubGlobal("fetch", fetchMock);
    await expect(readLane17ReplenishmentRows(...policies, env())).resolves.toEqual([{ id: "policy-1" }]);
    await expect(readLane17ReplenishmentRows(...spares, env())).resolves.toEqual([{ id: "policy-1" }]);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    for (const call of fetchMock.mock.calls) {
      expect((call as unknown as [URL, RequestInit])[0].toString()).toContain("/holdings/acme/");
      expect((call as unknown as [URL, RequestInit])[1].headers).toMatchObject({ host: "acme.jesadakorn.com" });
      expect(String((call as unknown as [URL, RequestInit])[1].body)).not.toMatch(/SELECT|FROM |WHERE /i);
    }
  });

  test("replenishment reads fail closed for company mismatch, unknown kind and 5xx", async () => {
    const fetchMock = vi.fn(async () => ok([]));
    vi.stubGlobal("fetch", fetchMock);
    await expect(readLane17ReplenishmentRows(LANE17_RD_REORDER_POLICIES, ["company-2", null, null], ACTOR, env())).rejects.toMatchObject({ status: 503 });
    await expect(readLane17Rows("service_spares_write", ["company-1", null], ACTOR, env())).rejects.toMatchObject({ status: 503 });
    await expect(readLane17ReplenishmentRows(LANE17_RD_SERVICE_SPARES, ["company-1"], ACTOR, env())).rejects.toMatchObject({ status: 503 });
    vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 503 })));
    await expect(readLane17ReplenishmentRows(...spares, env())).rejects.toMatchObject({ status: 503 });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  test("a changed Holding context cannot fall back to the first holding", async () => {
    const fetchMock = vi.fn(async () => ok([]));
    vi.stubGlobal("fetch", fetchMock);
    await expect(
      readLane17ReplenishmentRows(policies[0], policies[1], { ...ACTOR, holdingId: "h999" }, env()),
    ).rejects.toMatchObject({ status: 409, code: "WORK_CONTEXT_CHANGED" });
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("native lane 17 slice evidence", () => {
  const adapters = [
    "inventory-landed-costs.ts",
    "inventory-reorder-policies.ts",
    "inventory-consumption.ts",
  ].map((name) => ({
    name,
    source: readFileSync(`${ROOT}/packages/server-inventory/src/adapters/${name}`, "utf8"),
  }));
  const rust = readFileSync(`${ROOT}/packages/server-inventory/src/adapters/native_lane_17/postgres.rs`, "utf8");

  test("every replaced TypeScript adapter retains no SQL statement", () => {
    for (const { name, source } of adapters) {
      expect(source.match(/SELECT /g) ?? [], name).toHaveLength(0);
      expect(source.match(/\.prepare\(/g) ?? [], name).toHaveLength(0);
      expect(source, name).toContain("native-lane-17/client");
    }
  });

  test("Rust owner SQL hashes pin the byte-identical rendered statements", () => {
    const statements = [...rust.matchAll(/const (SQL_\w+): &str = r#"([\s\S]*?)"#;/g)];
    expect(statements).toHaveLength(6);
    const hashes = Object.fromEntries(
      statements.map(([, name, sql]) => [name, createHash("sha256").update(sql).digest("hex")]),
    );
    expect(hashes).toEqual({
      SQL_SCOPED: "070942f7fbb48d6ae399623d3dd8c1d515b7505b8fce0dd426a15304a183621b",
      SQL_FOR_ORDER: "c1ba5b9e9acba1646f1d21f07eef84b67a315f5f5696586e9f22915aaae735f9",
      SQL_MOVED_STOCK: "9abe06c83423ee185c42b6497e78c2fdb86b927bd7e1fc2239a158049dabcea4",
      SQL_WORK_ROWS: "41fb758c4c568aca7436e25c239e2d8c60b7fc999150af07479d5691858c448a",
      SQL_REORDER_POLICIES: "b0e831a4f9960bd0c7d27fcb121c45373d7714ba54d47ee2fe6394c4a13a0cab",
      SQL_SERVICE_SPARES: "32d473fafcdd9f247f0f1ae01175fc9dac9b9047e96c69724d7feefb65082e83",
    });
  });

  test("owner TS SELECT token KPI is 285 after the lane 17 replenishment slice", () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".ts")) total += (readFileSync(path, "utf8").match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(ROOT, "packages/server-inventory/src"));
    expect(total).toBe(285);
  });
});
