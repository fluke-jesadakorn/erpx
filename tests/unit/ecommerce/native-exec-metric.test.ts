import { describe, expect, test, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import "server-only";

const CONFIG = JSON.stringify({
  privateBaseUrl: "http://127.0.0.1:44777",
  timeoutMs: 2000,
  holdings: [
    { origin: "https://acme.jesadakorn.com", host: "acme.jesadakorn.com", holdingId: "h1", holdingSlug: "acme" },
  ],
});
const ACTOR = { holdingId: "h1", holdingSlug: "acme" };
const REQUEST = {
  companyId: "co-1",
  branchId: null,
  departmentId: null,
  metric: "open-ecommerce-carts",
  breakdown: "status",
  from: "2026-01-01",
  to: "2026-01-31",
} as const;
const SQL =
  "SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) AND c.status='open' GROUP BY 1 ORDER BY 1";
const PAYLOAD = { raw: [{ label: "open", value: "3" }], sql: SQL, values: ["co-1", null, null, null, null] };

const reply = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json" } });

const { createExecMetricEcomClient } = await import(
  "../../../packages/server-ecommerce/src/adapters/native-exec-metric/client"
);

describe("native Ecommerce executive metric transport", () => {
  test("sends only routing facts and decodes the trusted statement and scalar rows", async () => {
    let seen: { url: URL; init: RequestInit } | undefined;
    const transport = vi.fn(async (url: URL, init: RequestInit) => {
      seen = { url, init };
      return reply(PAYLOAD);
    });
    const client = createExecMetricEcomClient(CONFIG, transport);
    const snapshot = await client.projection(REQUEST, ACTOR);
    expect(snapshot).toEqual(PAYLOAD);
    expect(seen!.url.pathname).toBe("/api/v1/holdings/acme/ecommerce/native-exec-metric/projection");
    expect(seen!.url.origin).toBe("http://127.0.0.1:44777");
    const headers = seen!.init.headers as Headers;
    expect(headers.get("host")).toBe("acme.jesadakorn.com");
    expect(headers.get("origin")).toBe("https://acme.jesadakorn.com");
    expect(headers.get("x-erpx-context-company")).toBe("co-1");
    expect(headers.has("x-erpx-workspace-branch")).toBe(false);
    expect(seen!.init.body).toBe(
      JSON.stringify({ metric: "open-ecommerce-carts", breakdown: "status", from: Date.parse("2026-01-01"), to: Date.parse("2026-01-31") + 86_400_000 }),
    );
  });

  test("forwards the optional Branch and Department selectors", async () => {
    let headers = new Headers();
    const client = createExecMetricEcomClient(CONFIG, async (_url, init) => {
      headers = init.headers as Headers;
      return reply(PAYLOAD);
    });
    await client.projection({ ...REQUEST, branchId: "b-1", departmentId: "d-1" }, ACTOR);
    expect(headers.get("x-erpx-workspace-branch")).toBe("b-1");
    expect(headers.get("x-erpx-workspace-department")).toBe("d-1");
  });

  test("rejects drifted SQL, malformed rows and bind-count mismatch", async () => {
    for (const sql of [
      `${SQL}; DROP TABLE ecommerce_carts`,
      SQL.replace("FROM ecommerce_carts", "FROM ecommerce_secret"),
      `${SQL} SELECT 1`,
      SQL,
    ]) {
      const values = sql === SQL ? ["co-1", null] : PAYLOAD.values;
      const client = createExecMetricEcomClient(CONFIG, async () => reply({ ...PAYLOAD, sql, values }));
      await expect(client.projection(REQUEST, ACTOR)).rejects.toThrow();
    }
    const extra = createExecMetricEcomClient(CONFIG, async () =>
      reply({ ...PAYLOAD, raw: [{ label: "open", value: "3", extra: 1 }] }),
    );
    await expect(extra.projection(REQUEST, ACTOR)).rejects.toThrow();
    const extraKey = createExecMetricEcomClient(CONFIG, async () => reply({ ...PAYLOAD, injected: true }));
    await expect(extraKey.projection(REQUEST, ACTOR)).rejects.toThrow();
  });

  test("fails closed on missing config, reserved ports, unknown metric and unmatched Holding", async () => {
    expect(() => createExecMetricEcomClient(undefined)).toThrow();
    expect(() =>
      createExecMetricEcomClient(CONFIG.replace("44777", "3100")),
    ).toThrow();
    const client = createExecMetricEcomClient(CONFIG, async () => reply(PAYLOAD));
    await expect(client.projection({ ...REQUEST, metric: "receivables" }, ACTOR)).rejects.toThrow();
    await expect(client.projection(REQUEST, { holdingId: "other", holdingSlug: "acme" })).rejects.toThrow();
  });

  test("maps owner 4xx errors and 5xx transport failures", async () => {
    const rejected = createExecMetricEcomClient(CONFIG, async () =>
      reply({ error: { code: "ECOMMERCE_EXEC_METRIC_REQUEST_INVALID", message: "bad scope" } }, 422),
    );
    await expect(rejected.projection(REQUEST, ACTOR)).rejects.toMatchObject({
      status: 422,
      code: "ECOMMERCE_EXEC_METRIC_REQUEST_INVALID",
    });
    const down = createExecMetricEcomClient(CONFIG, async () => reply({}, 503));
    await expect(down.projection(REQUEST, ACTOR)).rejects.toMatchObject({ status: 503 });
  });
});

describe("native Ecommerce executive metric Rust parity", () => {
  const rust = readFileSync(
    resolve(process.cwd(), "packages/server-ecommerce/src/adapters/native_exec_metric/domain.rs"),
    "utf8",
  );

  test("the Rust oracle owns the four retired count statements byte-for-byte", () => {
    expect(rust).toContain(
      "SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE {scope_sql} AND c.status='open' GROUP BY 1 ORDER BY 1",
    );
    expect(rust).toContain(
      "SELECT x.status label,count(*)::text value FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id WHERE {scope_sql} AND x.status IN ('confirmed','partially_fulfilled','fulfilled') AND x.created_at>=? AND x.created_at<? GROUP BY 1 ORDER BY 1",
    );
    expect(rust).toContain(
      "SELECT r.status label,count(*)::text value FROM ecommerce_return_links r JOIN ecommerce_checkouts x ON x.id=r.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE {scope_sql} AND r.created_at>=? AND r.created_at<? GROUP BY 1 ORDER BY 1",
    );
    expect(rust).toContain(
      "SELECT l.status label,count(*)::text value FROM ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON x.id=l.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id WHERE {scope_sql} AND l.created_at>=? AND l.created_at<? GROUP BY 1 ORDER BY 1",
    );
  });

  test("the Rust oracle owns the branch and department organisation labels", () => {
    expect(rust).toContain("node_name_label(\"branch_id\", \"Branch\")");
    expect(rust).toContain("node_name_label(\"department_id\", \"Unassigned\")");
    expect(rust).toContain("FROM ({ORG_NODES_SOURCE}) n WHERE n.id=c.{column} AND n.company_id=c.company_id");
    expect(rust).toContain('SELECT \\"id\\",\\"kind\\",\\"slug\\",\\"display_name\\",\\"parent_id\\",\\"company_id\\",\\"branch_id\\",\\"status\\",\\"hierarchy_revision\\" FROM organization_nodes');
  });

  test("the TS family no longer owns any SQL text", () => {
    const src = readFileSync(
      resolve(process.cwd(), "packages/server-ecommerce/src/adapters/executive-metric-projections.ts"),
      "utf8",
    );
    expect(src).not.toContain("ecommerceScalar");
    expect(src).not.toContain("countBy");
    expect(src).not.toContain("nodeNameLabel");
    expect(src).not.toContain("SELECT ");
    expect(src).toContain("nativeExecutiveMetric");
  });
});
