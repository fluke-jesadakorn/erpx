import { describe, it, expect, vi } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { resolve } from "node:path";

vi.mock("server-only", () => ({}));
import {
  createLane39EcomClient,
  nativeLane39EcomConfigured,
  type Lane39HoldingContext,
  type NativeLane39Transport,
} from "../../../packages/server-ecommerce/src/adapters/native-lane-39/client";

/** Exact dev value for ERPX_NATIVE_ECOM_LANE39_CONFIG (1..64 holdings). */
export const DEV_CONFIG = {
  privateBaseUrl: "http://127.0.0.1:3198",
  timeoutMs: 10000,
  holdings: [
    {
      origin: "https://acme.jesadakorn.com",
      host: "acme.jesadakorn.com",
      holdingId: "h001",
      holdingSlug: "acme",
    },
    {
      origin: "https://northstar.jesadakorn.com",
      host: "northstar.jesadakorn.com",
      holdingId: "h002",
      holdingSlug: "northstar",
    },
    {
      origin: "https://erpx-internal.jesadakorn.com",
      host: "erpx-internal.jesadakorn.com",
      holdingId: "h003",
      holdingSlug: "erpx",
    },
  ],
} as const;

const raw = JSON.stringify(DEV_CONFIG);
const acme: Lane39HoldingContext = { holdingId: "h001", holdingSlug: "acme" };
const northstar: Lane39HoldingContext = { holdingId: "h002", holdingSlug: "northstar" };
const erpx: Lane39HoldingContext = { holdingId: "h003", holdingSlug: "erpx" };
const scope = { companyId: "co-1", branchId: "branch-1", departmentId: null };

const channel = {
  id: "channel-1", company_id: "co-1", name: "Web", channel_type: "web",
  configuration_json: "{}", status: "active", created_by: "user-1", reviewed_by: null,
  version: 1, created_at: 1, updated_at: 2,
};
const cart = {
  id: "cart-1", company_id: "co-1", branch_id: "branch-1", department_id: null,
  channel_id: "channel-1", party_id: "party-1", warehouse_id: "warehouse-1", currency: "THB",
  status: "open", created_by: "user-1", version: 1, created_at: 1, updated_at: 2,
};
const lists = { channels: [channel], carts: [cart] };
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), { status: 200, headers: { "content-type": "application/json" } });
const sha256 = (value: string) => createHash("sha256").update(value).digest("hex");

describe("lane 39 Ecommerce native workspace-list transport", () => {
  it("selects the exact WorkContext holding and forwards Company/Branch scopes", async () => {
    const transport: NativeLane39Transport = vi.fn(async (url, init) => {
      expect(url.toString()).toBe(
        "http://127.0.0.1:3198/api/v1/holdings/acme/ecommerce/lane39-workspace-lists",
      );
      expect(init.method).toBe("POST");
      const headers = init.headers as Headers;
      expect(headers.get("host")).toBe("acme.jesadakorn.com");
      expect(headers.get("origin")).toBe("https://acme.jesadakorn.com");
      expect(headers.get("x-erpx-context-company")).toBe("co-1");
      expect(headers.get("x-erpx-workspace-branch")).toBe("branch-1");
      expect(headers.get("x-erpx-workspace-department")).toBeNull();
      return ok(lists);
    });
    expect(await createLane39EcomClient(raw, transport).workspaceLists(scope, acme)).toEqual(lists);
  });

  it("routes the other two registered holdings by holdingId + holdingSlug", async () => {
    for (const [actor, slug, host] of [
      [northstar, "northstar", "northstar.jesadakorn.com"],
      [erpx, "erpx", "erpx-internal.jesadakorn.com"],
    ] as const) {
      const transport: NativeLane39Transport = vi.fn(async (url, init) => {
        expect(url.toString()).toBe(
          `http://127.0.0.1:3198/api/v1/holdings/${slug}/ecommerce/lane39-workspace-lists`,
        );
        expect((init.headers as Headers).get("host")).toBe(host);
        return ok(lists);
      });
      expect(await createLane39EcomClient(raw, transport).workspaceLists(scope, actor)).toEqual(lists);
    }
  });

  it("never falls back to the first holding for an unmatched or absent WorkContext", async () => {
    const transport: NativeLane39Transport = vi.fn(async () => ok(lists));
    const client = createLane39EcomClient(raw, transport);
    for (const actor of [undefined, { holdingId: "h404", holdingSlug: "acme" }, { holdingId: "h001", holdingSlug: "northstar" }]) {
      await expect(client.workspaceLists(scope, actor)).rejects.toMatchObject({
        status: 503,
        code: "ECOMMERCE_WORKSPACE_LIST_TARGET_UNAVAILABLE",
      });
    }
    expect(transport).not.toHaveBeenCalled();
  });

  it("refuses drifted channel/cart rows and malformed envelopes", async () => {
    const bad = [
      { ...lists, channels: [{ ...channel, status: "unknown" }] },
      { ...lists, carts: [{ ...cart, unexpected: true }] },
      { ...lists, channels: "not-an-array" },
    ];
    for (const body of bad) {
      const client = createLane39EcomClient(raw, vi.fn(async () => ok(body)));
      await expect(client.workspaceLists(scope, acme)).rejects.toMatchObject({
        status: 503,
        code: "ECOMMERCE_WORKSPACE_LIST_RESPONSE_INVALID",
      });
    }
  });

  it("fails closed when unconfigured, unreachable, rejected, or malformed", async () => {
    expect(nativeLane39EcomConfigured({})).toBe(false);
    expect(() => createLane39EcomClient(undefined, vi.fn())).toThrowError(
      expect.objectContaining({ status: 503, code: "ECOMMERCE_WORKSPACE_LIST_UNAVAILABLE" }),
    );
    const unreachable = createLane39EcomClient(raw, vi.fn(async () => { throw new Error("offline"); }));
    await expect(unreachable.workspaceLists(scope, acme)).rejects.toMatchObject({
      status: 503, code: "ECOMMERCE_WORKSPACE_LIST_UNAVAILABLE",
    });
    const rejected = createLane39EcomClient(
      raw,
      vi.fn(async () =>
        new Response(JSON.stringify({ error: { code: "ECOMMERCE_WORKSPACE_LIST_REQUEST_INVALID", message: "bounded scope" } }), {
          status: 422, headers: { "content-type": "application/json" },
        }),
      ),
    );
    await expect(rejected.workspaceLists(scope, acme)).rejects.toMatchObject({
      status: 422, code: "ECOMMERCE_WORKSPACE_LIST_REQUEST_INVALID",
    });
    const malformed = createLane39EcomClient(
      raw,
      vi.fn(async () => new Response("not json", { status: 200, headers: { "content-type": "application/json" } })),
    );
    await expect(malformed.workspaceLists(scope, acme)).rejects.toMatchObject({
      status: 503, code: "ECOMMERCE_WORKSPACE_LIST_RESPONSE_INVALID",
    });
  });

  it("rejects 0 or 65 holdings and duplicate targets", () => {
    expect(() => createLane39EcomClient(JSON.stringify({ ...DEV_CONFIG, holdings: [] }), vi.fn())).toThrow();
    expect(() =>
      createLane39EcomClient(
        JSON.stringify({ ...DEV_CONFIG, holdings: Array.from({ length: 65 }, () => DEV_CONFIG.holdings[0]) }),
        vi.fn(),
      ),
    ).toThrow();
    expect(() =>
      createLane39EcomClient(
        JSON.stringify({ ...DEV_CONFIG, holdings: [DEV_CONFIG.holdings[0], DEV_CONFIG.holdings[0]] }),
        vi.fn(),
      ),
    ).toThrow();
  });

  it("pins the final owner SELECT KPI: ecommerce 1, brand 2", () => {
    for (const [owner, expected] of [["server-ecommerce", 0], ["server-brand", 2]] as const) {
      let total = 0;
      const walk = (dir: string) => {
        for (const entry of readdirSync(dir, { withFileTypes: true })) {
          const path = resolve(dir, entry.name);
          if (entry.isDirectory()) walk(path);
          else if (entry.name.endsWith(".ts"))
            total += (readFileSync(path, "utf8").match(/SELECT /g) ?? []).length;
        }
      };
      walk(resolve(process.cwd(), `packages/${owner}/src`));
      expect(total, owner).toBe(expected);
    }
    expect(
      (readFileSync(
        resolve(process.cwd(), "packages/server-ecommerce/src/adapters/ecommerce.ts"),
        "utf8",
      ).match(/readSource\("ecommerce_channels"|ecommerceOrderedList\("c\.\*", "ecommerce_carts c"/g) ?? []).length,
    ).toBe(0);
  });

  it("pins both rendered SQL/result oracles against the Rust adapter literals", () => {
    const source = readFileSync(
      resolve(process.cwd(), "packages/server-ecommerce/src/adapters/native_lane_39/postgres.rs"),
      "utf8",
    );
    const channels =
      "SELECT * FROM ecommerce_channels WHERE company_id=$1 ORDER BY updated_at DESC";
    const carts =
      "SELECT c.* FROM ecommerce_carts c WHERE c.company_id=$1 AND ($2::text IS NULL OR c.branch_id=$2) \
       AND ($3::text IS NULL OR c.department_id=$3) ORDER BY c.updated_at DESC"
        .replace(/\s+/g, " ")
        .trim();
    const literal = (name: string) => {
      const match = source.match(new RegExp(`pub const ${name}: &str =\\s*"([\\s\\S]*?)";`));
      expect(match).not.toBeNull();
      return match![1]
        .replace(/\\\s*\n\s*/g, " ")
        .replace(/\s+/g, " ")
        .trim();
    };
    expect(literal("CHANNELS_SQL")).toBe(channels);
    expect(literal("CARTS_SQL")).toBe(carts);
    expect(sha256(channels)).toBe("1c3af3b5bee78a1e340a8c2f34d353c140a1e53b0de7831d319ecdbec2dbda55");
    expect(sha256(carts)).toBe("6e5f4c105511f740a5343577b1aac08e23769f28c606bc7c834c9a7ee4609b49");
    expect(sha256(JSON.stringify(lists))).toBe(
      "b7dc2a57cc24e2c01b4eacb330b9e3e7a59142a5617783502e260210aaa01930",
    );
  });
});
