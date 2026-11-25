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
import {
  createLane39EvidenceClient,
  nativeLane39EvidenceConfigured,
  type EvidenceScope,
} from "../../../packages/server-brand/src/adapters/native-lane-39/evidence-client";

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

  it("pins the brand owner SELECT KPI at the count after the r2 guard cutover (1)", () => {
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
    expect(total).toBe(1);
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

describe("lane 39 Brand native evidence transport", () => {
  const escope: EvidenceScope = {
    companyId: "co-1", branchId: "br-1",
    from: 1, toExclusive: 2, query: "north",
    cursor: { branchName: "North", id: "profile-1" }, limit: 26,
  };
  const evidenceRows = [{
    id: "profile-1", branch_name: "North", document_mode: "branch", updated_at: 7,
    primary_attachment_id: "image-1", inverse_attachment_id: null,
  }];
  const eok = (value: unknown) =>
    new Response(JSON.stringify(value), {
      status: 200, headers: { "content-type": "application/json" },
    });
  const expectedBody = {
    companyId: "co-1", branchId: "br-1", from: 1, toExclusive: 2, query: "north",
    cursor: { branchName: "North", id: "profile-1" }, limit: 26,
  };

  it("selects the exact WorkContext holding and pins the owner request and row shape", async () => {
    const transport: NativeLane39Transport = vi.fn(async (url, init) => {
      expect(url.toString()).toBe(
        "http://127.0.0.1:3198/api/v1/holdings/acme/branding/lane39-evidence-search",
      );
      expect(init.method).toBe("POST");
      expect(String(init.body)).toBe(JSON.stringify(expectedBody));
      const headers = init.headers as Headers;
      expect(headers.get("host")).toBe("acme.jesadakorn.com");
      expect(headers.get("origin")).toBe("https://acme.jesadakorn.com");
      expect(headers.get("x-erpx-context-company")).toBe("co-1");
      return eok({ rows: evidenceRows });
    });
    const client = createLane39EvidenceClient(raw, transport);
    expect(await client.search(escope, acme)).toEqual(evidenceRows);
  });

  it("routes the other two registered holdings by holdingId + holdingSlug", async () => {
    for (const [actor, slug, host] of [
      [northstar, "northstar", "northstar.jesadakorn.com"],
      [erpx, "erpx", "erpx-internal.jesadakorn.com"],
    ] as const) {
      const transport: NativeLane39Transport = vi.fn(async (url, init) => {
        expect(url.toString()).toBe(
          `http://127.0.0.1:3198/api/v1/holdings/${slug}/branding/lane39-evidence-search`,
        );
        expect((init.headers as Headers).get("host")).toBe(host);
        return eok({ rows: evidenceRows });
      });
      expect(await createLane39EvidenceClient(raw, transport).search(escope, actor)).toEqual(evidenceRows);
    }
  });

  it("never falls back to the first holding for an unmatched or absent WorkContext", async () => {
    const transport: NativeLane39Transport = vi.fn(async () => eok({ rows: evidenceRows }));
    const client = createLane39EvidenceClient(raw, transport);
    for (const actor of [undefined, { holdingId: "h404", holdingSlug: "acme" }, { holdingId: "h001", holdingSlug: "northstar" }]) {
      await expect(client.search(escope, actor)).rejects.toMatchObject({
        status: 503, code: "BRAND_EVIDENCE_SEARCH_TARGET_UNAVAILABLE",
      });
    }
    expect(transport).not.toHaveBeenCalled();
  });

  it("rejects drifted evidence rows, wrong predicate pairs and malformed envelopes", async () => {
    const badRows = [
      { rows: [{ ...evidenceRows[0], document_mode: "unknown" }] },
      { rows: [{ ...evidenceRows[0], unexpected: true }] },
      { rows: [{ ...evidenceRows[0], updated_at: -1 }] },
      { rows: "not-an-array" },
    ];
    for (const body of badRows) {
      const client = createLane39EvidenceClient(raw, vi.fn(async () => eok(body)));
      await expect(client.search(escope, acme)).rejects.toMatchObject({
        status: 503, code: "BRAND_EVIDENCE_SEARCH_RESPONSE_INVALID",
      });
    }
    const client = createLane39EvidenceClient(raw, vi.fn(async () => eok({ rows: evidenceRows })));
    await expect(client.search({ ...escope, from: 1, toExclusive: undefined }, acme)).rejects.toMatchObject({
      status: 422, code: "BRAND_EVIDENCE_SEARCH_REQUEST_INVALID",
    });
    await expect(client.search({ ...escope, limit: 52 }, acme)).rejects.toMatchObject({
      status: 422, code: "BRAND_EVIDENCE_SEARCH_REQUEST_INVALID",
    });
  });

  it("fails closed when unconfigured, unreachable, rejected or malformed", async () => {
    expect(nativeLane39EvidenceConfigured({})).toBe(false);
    expect(() => createLane39EvidenceClient(undefined, vi.fn())).toThrowError(
      expect.objectContaining({ status: 503, code: "BRAND_EVIDENCE_SEARCH_UNAVAILABLE" }),
    );
    const unreachable = createLane39EvidenceClient(raw, vi.fn(async () => { throw new Error("offline"); }));
    await expect(unreachable.search(escope, acme)).rejects.toMatchObject({
      status: 503, code: "BRAND_EVIDENCE_SEARCH_UNAVAILABLE",
    });
    const rejected = createLane39EvidenceClient(
      raw,
      vi.fn(async () =>
        new Response(JSON.stringify({ error: { code: "BRAND_EVIDENCE_SEARCH_REQUEST_INVALID", message: "bounded search" } }), {
          status: 422, headers: { "content-type": "application/json" },
        }),
      ),
    );
    await expect(rejected.search(escope, acme)).rejects.toMatchObject({
      status: 422, code: "BRAND_EVIDENCE_SEARCH_REQUEST_INVALID",
    });
    const malformed = createLane39EvidenceClient(
      raw, vi.fn(async () => new Response("not json", { status: 200, headers: { "content-type": "application/json" } })),
    );
    await expect(malformed.search(escope, acme)).rejects.toMatchObject({
      status: 503, code: "BRAND_EVIDENCE_SEARCH_RESPONSE_INVALID",
    });
  });

  it("pins the TS-statement retirement and the exact owner SQL head oracle", () => {
    const owner = readFileSync(
      resolve(process.cwd(), "packages/server-brand/src/adapters/executive-evidence-sources.ts"),
      "utf8",
    );
    expect(owner).not.toMatch(/planBrandRead|BRAND_RD_EVIDENCE_SEARCH|db\.prepare|SELECT /);
    expect(owner).toContain("nativeBrandEvidence");
    const domain = readFileSync(
      resolve(process.cwd(), "packages/server-brand/src/adapters/native_lane_39/evidence_domain.rs"),
      "utf8",
    );
    expect(domain).toContain("pub const MAX_LIMIT: i64 = 51;");
    const source = readFileSync(
      resolve(process.cwd(), "packages/server-brand/src/adapters/native_lane_39/evidence_postgres.rs"),
      "utf8",
    );
    expect(source).toMatch(/plan_brand_read\(\s*RD_EVIDENCE_SEARCH,/);
    expect(source).toContain("query.into_sql()");
    const head = source.match(/const HEAD: &str = "([^"]+)";/)![1];
    expect(head).toBe(
      "SELECT p.scope_id id,b.display_name branch_name,p.document_mode,p.updated_at," +
      "p.primary_attachment_id,p.inverse_attachment_id FROM organization_brand_profiles p JOIN (",
    );
    expect(sha256(head)).toBe("ce6db0fecf34242f4394cd6476d14c22b12981565f3622e43431bffe18070a2f");
    expect(sha256(JSON.stringify(evidenceRows))).toBe(
      "70bca29d3117a2a7d6ef3041e14db72717c10818999f5eabef0f71cc3c99870a",
    );
  });
});
