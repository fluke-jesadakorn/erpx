import { describe, expect, test, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import "server-only";

const CONFIG = JSON.stringify({
  privateBaseUrl: "http://127.0.0.1:44779",
  timeoutMs: 2000,
  holdings: [
    { origin: "https://acme.jesadakorn.com", host: "acme.jesadakorn.com", holdingId: "h1", holdingSlug: "acme" },
  ],
});
const ACTOR = { holdingId: "h1", holdingSlug: "acme" };
const SCOPE = { companyId: "co-1", branchId: null, departmentId: null };
const PAYLOAD = {
  listings: [{ id: "l-1", company_id: "co-1", status: "published", version: 1, product_name: "Tea", channel_name: "Web" }],
  checkouts: [{ id: "x-1", company_id: "co-1", status: "confirmed", number: "SO-1", order_state: "approved" }],
};
const reply = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json" } });

const { createLane39EcomClient } = await import(
  "../../../packages/server-ecommerce/src/adapters/native-lane-39/client"
);

describe("native lane 39 workspace row transport", () => {
  test("posts the scope and decodes opaque owner rows without emitting SQL", async () => {
    let seen: { url: URL; init: RequestInit } | undefined;
    const client = createLane39EcomClient(CONFIG, async (url, init) => {
      seen = { url, init };
      return reply(PAYLOAD);
    });
    const rows = await client.workspaceRows(SCOPE, ACTOR);
    expect(rows).toEqual(PAYLOAD);
    expect(seen!.url.pathname).toBe("/api/v1/holdings/acme/ecommerce/lane39-workspace-rows");
    const headers = seen!.init.headers as Headers;
    expect(headers.get("host")).toBe("acme.jesadakorn.com");
    expect(headers.get("origin")).toBe("https://acme.jesadakorn.com");
    expect(headers.get("x-erpx-context-company")).toBe("co-1");
    expect(headers.has("x-erpx-workspace-branch")).toBe(false);
    expect(seen!.init.body).toBeUndefined();
  });

  test("forwards Branch and Department selectors", async () => {
    let headers = new Headers();
    const client = createLane39EcomClient(CONFIG, async (_url, init) => {
      headers = init.headers as Headers;
      return reply(PAYLOAD);
    });
    await client.workspaceRows({ ...SCOPE, branchId: "b-1", departmentId: "d-1" }, ACTOR);
    expect(headers.get("x-erpx-workspace-branch")).toBe("b-1");
    expect(headers.get("x-erpx-workspace-department")).toBe("d-1");
  });

  test("rejects drifted rows, extra reply keys and bad scope", async () => {
    for (const payload of [
      { ...PAYLOAD, injected: true },
      { listings: [{ ...PAYLOAD.listings[0], active: true }], checkouts: [] },
      { listings: [{ id: "l-1", company_id: 5 }], checkouts: [] },
      { listings: [{ ...PAYLOAD.listings[0], id: "bad id" }], checkouts: [] },
    ]) {
      const client = createLane39EcomClient(CONFIG, async () => reply(payload));
      await expect(client.workspaceRows(SCOPE, ACTOR)).rejects.toThrow();
    }
    const client = createLane39EcomClient(CONFIG, async () => reply(PAYLOAD));
    await expect(client.workspaceRows({ ...SCOPE, companyId: "" }, ACTOR)).rejects.toThrow();
    await expect(client.workspaceRows(SCOPE, { holdingId: "other", holdingSlug: "acme" })).rejects.toThrow();
  });

  test("maps owner 4xx and transport 5xx failures", async () => {
    const rejected = createLane39EcomClient(CONFIG, async () =>
      reply({ error: { code: "ECOMMERCE_WORKSPACE_ROW_REQUEST_INVALID", message: "bad scope" } }, 422),
    );
    await expect(rejected.workspaceRows(SCOPE, ACTOR)).rejects.toMatchObject({ status: 422, code: "ECOMMERCE_WORKSPACE_ROW_REQUEST_INVALID" });
    const down = createLane39EcomClient(CONFIG, async () => reply({}, 503));
    await expect(down.workspaceRows(SCOPE, ACTOR)).rejects.toMatchObject({ status: 503 });
  });
});

describe("workspace row Rust/TS parity", () => {
  const srcRoot = resolve(__dirname, "../../../packages/server-ecommerce/src");
  const rust = readFileSync(resolve(srcRoot, "adapters/native_lane_39/workspace_rows.rs"), "utf8");
  const consumer = readFileSync(resolve(srcRoot, "adapters/ecommerce.ts"), "utf8");

  test("the Rust oracle owns the two retired parameterized statements", () => {
    expect(rust).toContain("SELECT l.*,p.name product_name,c.name channel_name FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN (");
    expect(rust).toContain("WHERE c.company_id=? ORDER BY l.updated_at DESC");
    expect(rust).toContain("SELECT x.*,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (");
    expect(rust).toContain("WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY x.updated_at DESC");
    expect(rust).toContain("FROM commerce_products");
    expect(rust).toContain("FROM commerce_orders");
  });

  test("the TS workspace consumer issues no SQL for listings or checkouts", () => {
    expect(consumer).toContain("nativeWorkspaceRows(scope, a)");
    expect(consumer).not.toContain("p.name product_name,c.name channel_name");
    expect(consumer).not.toContain("x.updated_at DESC");
    expect(consumer).not.toContain("ecommerceOrderedList(\"l.*,p.name");
    expect(consumer).not.toContain("ecommerceOrderedList(\"x.*,o.number");
  });

  test("the family stays read-only and row-bounded", () => {
    expect(rust).toContain("pub const MAX_ROWS: usize = 5000");
    const statements = rust.slice(0, rust.indexOf("#[cfg(test)]"));
    expect(statements).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });
});
