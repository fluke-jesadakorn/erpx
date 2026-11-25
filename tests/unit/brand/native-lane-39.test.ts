import { describe, it, expect, vi } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { resolve } from "node:path";

vi.mock("server-only", () => ({}));
import {
  createLane39BrandClient,
  nativeLane39BrandConfigured,
  type Lane39HoldingContext,
  type NativeLane39Transport,
} from "../../../packages/server-brand/src/adapters/native-lane-39/client";

/** Exact dev value for ERPX_NATIVE_BRAND_LANE39_CONFIG (1..64 holdings). */
export const DEV_CONFIG = {
  privateBaseUrl: "http://127.0.0.1:3198",
  timeoutMs: 10000,
  holdings: [
    {
      origin: "https://acme.jesadakorn.com",
      host: "acme.jesadakorn.com",
      holdingId: "h001",
      holdingSlug: "acme",
      companyId: "acme-co",
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

const ok = (brands: unknown) =>
  new Response(JSON.stringify({ brands }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });

const rows = [
  { scope_kind: "holding", scope_id: "holding-1", primary_publication_id: "pub-1" },
  { scope_kind: "branch", scope_id: "branch-1", primary_publication_id: null },
];

const sha256 = (value: string) => createHash("sha256").update(value).digest("hex");

describe("lane 39 Brand native workspace-choice transport", () => {
  it("selects the exact WorkContext holding and pins target, Company set and response shape", async () => {
    const transport: NativeLane39Transport = vi.fn(async (url, init) => {
      expect(url.toString()).toBe(
        "http://127.0.0.1:3198/api/v1/holdings/acme/branding/lane39-workspace-choice",
      );
      expect(init.method).toBe("POST");
      expect(String(init.body)).toBe(JSON.stringify({ companyIds: ["co-a", "co-b"] }));
      const headers = init.headers as Headers;
      expect(headers.get("host")).toBe("acme.jesadakorn.com");
      expect(headers.get("origin")).toBe("https://acme.jesadakorn.com");
      expect(headers.get("x-erpx-context-company")).toBe("acme-co");
      return ok(rows);
    });
    const client = createLane39BrandClient(raw, transport);
    expect(await client.workspaceChoices(["co-a", "co-b"], acme)).toEqual(rows);
  });

  it("routes the other two registered holdings by holdingId + holdingSlug", async () => {
    for (const [actor, host, slug] of [
      [northstar, "northstar.jesadakorn.com", "northstar"],
      [erpx, "erpx-internal.jesadakorn.com", "erpx"],
    ] as const) {
      const transport: NativeLane39Transport = vi.fn(async (url, init) => {
        expect(url.toString()).toBe(
          `http://127.0.0.1:3198/api/v1/holdings/${slug}/branding/lane39-workspace-choice`,
        );
        expect((init.headers as Headers).get("host")).toBe(host);
        expect((init.headers as Headers).get("x-erpx-context-company")).toBeNull();
        return ok(rows);
      });
      expect(await createLane39BrandClient(raw, transport).workspaceChoices(["co-a"], actor)).toEqual(rows);
    }
  });

  it("never falls back to the first holding for an unmatched or absent WorkContext", async () => {
    const transport: NativeLane39Transport = vi.fn(async () => ok(rows));
    const client = createLane39BrandClient(raw, transport);
    for (const actor of [undefined, { holdingId: "h404", holdingSlug: "acme" }, { holdingId: "h001", holdingSlug: "northstar" }]) {
      await expect(client.workspaceChoices(["co-a"], actor)).rejects.toMatchObject({
        status: 503,
        code: "BRAND_WORKSPACE_CHOICE_TARGET_UNAVAILABLE",
      });
    }
    expect(transport).not.toHaveBeenCalled();
  });

  it("refuses an unregistered scope kind, extra field, or drifted row", async () => {
    const bad = [
      [{ ...rows[0], scope_kind: "department" }],
      [{ ...rows[0], unexpected: true }],
      [{ ...rows[0], scope_id: "bad id" }],
    ];
    for (const body of bad) {
      const client = createLane39BrandClient(
        raw,
        vi.fn(async () => ok(body)),
      );
      await expect(client.workspaceChoices(["co-a"], acme)).rejects.toMatchObject({
        status: 503,
        code: "BRAND_WORKSPACE_CHOICE_RESPONSE_INVALID",
      });
    }
  });

  it("fails closed when unconfigured, unreachable, rejected, or malformed", async () => {
    expect(nativeLane39BrandConfigured({})).toBe(false);
    expect(() => createLane39BrandClient(undefined, vi.fn())).toThrowError(
      expect.objectContaining({
        status: 503,
        code: "BRAND_WORKSPACE_CHOICE_UNAVAILABLE",
      }),
    );

    const unreachable = createLane39BrandClient(
      raw,
      vi.fn(async () => {
        throw new Error("connection refused");
      }),
    );
    await expect(unreachable.workspaceChoices(["co-a"], acme)).rejects.toMatchObject({
      status: 503,
      code: "BRAND_WORKSPACE_CHOICE_UNAVAILABLE",
    });

    const rejected = createLane39BrandClient(
      raw,
      vi.fn(async () =>
        new Response(JSON.stringify({ error: { code: "BRAND_WORKSPACE_CHOICE_REQUEST_INVALID", message: "bounded set" } }), {
          status: 422,
          headers: { "content-type": "application/json" },
        }),
      ),
    );
    await expect(rejected.workspaceChoices(["co-a"], acme)).rejects.toMatchObject({
      status: 422,
      code: "BRAND_WORKSPACE_CHOICE_REQUEST_INVALID",
    });

    const malformed = createLane39BrandClient(
      raw,
      vi.fn(async () =>
        new Response(JSON.stringify({ brands: "not-an-array" }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      ),
    );
    await expect(malformed.workspaceChoices(["co-a"], acme)).rejects.toMatchObject({
      status: 503,
      code: "BRAND_WORKSPACE_CHOICE_RESPONSE_INVALID",
    });
  });

  it("rejects 0 or 65 holdings and duplicate targets", () => {
    expect(() => createLane39BrandClient(JSON.stringify({ ...DEV_CONFIG, holdings: [] }), vi.fn())).toThrow();
    expect(() =>
      createLane39BrandClient(
        JSON.stringify({ ...DEV_CONFIG, holdings: Array.from({ length: 65 }, () => DEV_CONFIG.holdings[0]) }),
        vi.fn(),
      ),
    ).toThrow();
    expect(() =>
      createLane39BrandClient(
        JSON.stringify({ ...DEV_CONFIG, holdings: [DEV_CONFIG.holdings[0], DEV_CONFIG.holdings[0]] }),
        vi.fn(),
      ),
    ).toThrow();
  });

  it("pins the brand owner SELECT KPI at the final count of 2", () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".ts"))
          total += (readFileSync(path, "utf8").match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(process.cwd(), "packages/server-brand/src"));
    expect(total).toBe(2);
    expect(
      (readFileSync(
        resolve(process.cwd(), "packages/server-brand/src/adapters/workspace-choice-brand.ts"),
        "utf8",
      ).match(/SELECT /g) ?? []).length,
    ).toBe(0);
  });

  it("pins the rendered SQL/result oracle against the Rust adapter literal", () => {
    const source = readFileSync(
      resolve(process.cwd(), "packages/server-brand/src/adapters/native_lane_39/postgres.rs"),
      "utf8",
    );
    const sql =
      "SELECT scope_kind,scope_id,primary_publication_id FROM organization_brand_profiles " +
      "WHERE scope_kind='holding' OR company_id IN ($1, $2, $3)";
    const headLiteral = source.match(
      /"SELECT scope_kind,scope_id,primary_publication_id FROM organization_brand_profiles[\s\S]*?company_id IN \("/,
    );
    expect(headLiteral).toBeDefined();
    const head = headLiteral![0]
      .replace(/^"|"$/g, "")
      .replace(/\\\s*\n\s*/g, " ")
      .replace(/\s+/g, " ")
      .trim();
    const rendered = `${head}$1, $2, $3)`;
    expect(rendered).toBe(sql);
    expect(source).toContain("QueryBuilder::new(WORKSPACE_CHOICE_SQL_HEAD)");
    expect(sha256(rendered)).toBe(
      "faf1821d5a708d105445aa17732998ee019972fc8474dc58751ec6be92aee62d",
    );
    expect(sha256(JSON.stringify(rows))).toBe(
      "ea79543d73a2a2560cb8afd9c78076be065f74037e8e90fe2cc1c1fcf04c398b",
    );
  });
});
