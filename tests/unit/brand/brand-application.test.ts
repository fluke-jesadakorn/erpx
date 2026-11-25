import { describe, expect, test, it, vi, beforeEach, afterEach } from "vitest";
import { readFile } from "node:fs/promises";
vi.mock("server-only", () => ({}));
import { assertBrandCategoryLocks, assertBrandIndependentPublisher } from "@erpx/server-brand/domain/policy";
import { brandIdentitySchema, type ActiveBrandIdentity, type BrandCategoryLock, type BrandDraft } from "@erpx/server-brand/domain/identity";
import { readBrandIdentityWorkspace } from "@erpx/server-brand/application/identity-read";
import * as brandPublic from "@erpx/server-brand/public/brand-identity";
import * as brandIndex from "@erpx/server-brand/index";
import { nativeBrandIdentityWorkspace } from "@erpx/feature-brand/lib/native-brand-identity";

const identity = { identity: { shortName: "ERPX", tagline: "Approved identity" }, visual: { accentColor: "#224488", logoPublicationId: null } };
const reason = "Independently review the identity";
const draft: BrandDraft = { id: "draft-1", company_id: "company-1", based_revision: 0, identity_json: JSON.stringify(identity), status: "draft", version: 1, created_by: "maker", updated_by: "editor", updated_at: 10 };

describe("Retained Brand identity contract", () => {
  test("trims bounded identity values and normalizes the accent color", () => {
    const parsed = brandIdentitySchema.parse({ identity: { shortName: "  ERPX  ", tagline: "  Approved identity  " }, visual: { accentColor: "#AABBCC", logoPublicationId: null } });
    expect(parsed).toEqual({ identity: { shortName: "ERPX", tagline: "Approved identity" }, visual: { accentColor: "#aabbcc", logoPublicationId: null } });
    expect(brandIdentitySchema.parse({ identity: { shortName: "ERPX" }, visual: { accentColor: "#224488", logoPublicationId: "publication" } })).toMatchObject({ identity: { tagline: "" } });
  });
  test("rejects unknown fields and out-of-bounds identity values", () => {
    for (const invalid of [
      { ...identity, implementation: "hidden" },
      { identity: { shortName: "", tagline: "Approved" }, visual: identity.visual },
      { identity: { shortName: "x".repeat(41), tagline: "Approved" }, visual: identity.visual },
      { identity: { shortName: "ERPX", tagline: "x".repeat(161) }, visual: identity.visual },
      { identity: identity.identity, visual: { accentColor: "#22448", logoPublicationId: null } },
      { identity: identity.identity, visual: { accentColor: "#224488", logoPublicationId: "" } },
      { identity: identity.identity, visual: { accentColor: "#224488" } },
    ]) expect(brandIdentitySchema.safeParse(invalid).success).toBe(false);
  });
  test("compares approved category locks against canonical sections", () => {
    const current = brandIdentitySchema.parse(identity);
    const locks: BrandCategoryLock[] = (["identity", "visual"] as const).map(category => ({ category, locked: true, approved_snapshot_json: JSON.stringify(current[category]), version: 1, reason, updated_by: "owner", updated_at: 10 }));
    expect(() => assertBrandCategoryLocks(locks, current)).not.toThrow();
    expect(() => assertBrandCategoryLocks(locks, { ...current, visual: { ...current.visual, accentColor: "#778899" } })).toThrow(/visual/);
    expect(() => assertBrandCategoryLocks(locks, { ...current, identity: { ...current.identity, shortName: "Another name" } })).toThrow(/identity/);
    expect(() => assertBrandCategoryLocks([{ ...locks[0], locked: false }], { ...current, identity: { ...current.identity, shortName: "Another name" } })).not.toThrow();
  });
  test("requires another person for independent publication", () => {
    expect(() => assertBrandIndependentPublisher(id => id === "reviewer", "maker", "editor")).not.toThrow();
    for (const isSamePerson of [() => true, ((id:string) => id === "maker"), ((id:string) => id === "editor")]) {
      expect(() => assertBrandIndependentPublisher(isSamePerson, "maker", "editor")).toThrow(/another authorized person/i);
    }
  });
  test("parses a retained published identity and tolerates an unavailable lifecycle table", async () => {
    const row = { ...draft, revision: 1, draft_id: draft.id, draft_version: 2, published_by: "reviewer", published_at: 20, active_version: 3 } as unknown as ActiveBrandIdentity;
    expect(await readBrandIdentityPorts(true, row)).toMatchObject({ active_version: 3, identity });
    expect(await readBrandIdentityPorts(false, row)).toBeNull();
  });
  test("workspace view hides history and other drafts using provided authority facts", async () => {
    const output = await readBrandIdentityWorkspace({ actorId: "worker", canManage: false, canLock: false, async authorize() {}, async drafts(includeOthers) { expect(includeOthers).toBe(false); return []; }, async versions() { return []; }, async locks() { return []; }, async history() { return [{ id: "private" }] as never; }, async active() { return null; }, async legacy() { return { shortName: "Legacy", accentColor: "#123456" }; }, async media() { return []; } });
    expect(output).toMatchObject({ history: [], initial: { identity: { shortName: "Legacy", tagline: "" }, visual: { logoPublicationId: null } }, canManage: false });
  });
  test("retires every TypeScript identity writer and SQL reader from the public entrypoints", () => {
    for (const entrypoint of [brandPublic, brandIndex]) for (const writer of ["saveBrandIdentityDraft", "publishBrandIdentity", "rollbackBrandIdentity", "changeBrandCategoryLock", "cancelBrandDraft"])
      expect(Object.hasOwn(entrypoint, writer)).toBe(false);
    // Remaining public readers are native-backed delegates, never TS SELECTs.
    expect(typeof brandPublic.publishedBrandIdentity).toBe("function");
    expect(typeof brandPublic.brandIdentityWorkspace).toBe("function");
    expect(typeof nativeBrandIdentityWorkspace).toBe("function");
  });
});

async function readBrandIdentityPorts(available: boolean, row: ActiveBrandIdentity | null) {
  const { readPublishedBrandIdentity } = await import("@erpx/server-brand/application/identity-read");
  return readPublishedBrandIdentity({ async available() { return available; }, async active() { return row; } });
}
import { normalizeBrandMedia } from "@erpx/server-brand/application/branding";
describe("Retained Branding media normalization", () => {
  test("validates image metadata before encoding", async () => {
    const encoded = await normalizeBrandMedia({
      async metadata() { return { width: 64, height: 64 }; },
      async encode() { return new Uint8Array([1, 2, 3]); },
    }, new ArrayBuffer(0));
    expect(Array.from(encoded)).toEqual([1, 2, 3]);
  });
  test("maps decoder and dimension failures to retained logo codes", async () => {
    await expect(normalizeBrandMedia({
      async metadata() { throw new Error("bad decoder"); },
      async encode() { return new Uint8Array(); },
    }, new ArrayBuffer(0))).rejects.toMatchObject({ code: "LOGO_INVALID" });
    await expect(normalizeBrandMedia({
      async metadata() { return { width: 31, height: 32 }; },
      async encode() { throw new Error("must not encode"); },
    }, new ArrayBuffer(0))).rejects.toMatchObject({ code: "LOGO_DIMENSIONS" });
    await expect(normalizeBrandMedia({
      async metadata() { return { width: 4097, height: 4097 }; },
      async encode() { throw new Error("must not encode"); },
    }, new ArrayBuffer(0))).rejects.toMatchObject({ code: "LOGO_PIXELS" });
  });
  test("retains encoder failures without masking them", async () => {
    await expect(normalizeBrandMedia({
      async metadata() { return { width: 64, height: 64 }; },
      async encode() { throw new Error("encoder failure"); },
    }, new ArrayBuffer(0))).rejects.toThrow("encoder failure");
  });
});

describe("Brand residual read tail", () => {
  test("current-scope probe uses one SELECT token with a bound VALUES tuple", async () => {
    const source = await readFile(
      new URL("../../../packages/server-brand/src/adapters/attachment-read.ts", import.meta.url),
      "utf8",
    );
    expect(source.match(/SELECT/g)).toHaveLength(1);
    expect(source).toContain(
      "`SELECT 1 FROM(VALUES(?::text,?::text,?::text))workforce_actor_scope(company_id,branch_id,department_id) WHERE ${currentScope.where}`",
    );
    expect(source).not.toContain(
      "(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope",
    );
    expect(source).toContain(
      ".bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args)",
    );
  });
});

/** G4 / retire.brand.read-tail-r1: the three pure Brand read tails now plan on
 * the owner-local native read port. No TS SELECT literal remains in the cut
 * adapters; the authority guard in branding.ts and the cross-owner attachment
 * probe stay as classified residuals. */
describe("native Brand pure-read cutover", () => {
  const envKey = "ERPX_NATIVE_BRAND_READ_CONFIG";
  const original = process.env[envKey];
  const bodies: { kind: string; values: unknown[]; range?: boolean; query?: boolean; cursor?: boolean }[] = [];

  beforeEach(() => {
    bodies.length = 0;
    process.env[envKey] = JSON.stringify({
      privateBaseUrl: "http://127.0.0.1:44997",
      timeoutMs: 1000,
      holdings: [{ origin: "https://acme.example.com", holdingSlug: "acme", holdingId: "h001" }],
    });
    vi.stubGlobal("fetch", vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as typeof bodies[number];
      bodies.push(body);
      return new Response(JSON.stringify({ data: { sql: "SELECT 1 AS row", values: body.values } }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }));
  });
  afterEach(() => {
    if (original === undefined) delete process.env[envKey];
    else process.env[envKey] = original;
    vi.unstubAllGlobals();
  });

  const actor = {
    holdingId: "h001", holdingSlug: "acme", companyId: "company-1", branchId: "branch-1",
    assignmentId: "assignment-1", scope: { kind: "branch", branchSlug: "north" },
  } as never;
  const fakeDb = () => {
    const prepared: string[] = [];
    const bound: unknown[][] = [];
    const db = {
      prepare(sql: string) {
        prepared.push(sql);
        return {
          bind(...values: unknown[]) { bound.push(values); return this; },
          async all() { return { results: [{ row: 1 }], success: true, meta: {} }; },
        };
      },
    };
    return { db, prepared, bound };
  };

  it("exposes only SQL-free read discriminators", async () => {
    const ports = await import("../../../packages/server-brand/src/adapters/native-brand-read-ports");
    for (const kind of [ports.BRAND_RD_EVIDENCE_SEARCH, ports.BRAND_RD_METRIC_PROFILES_LABEL,
      ports.BRAND_RD_METRIC_PROFILES_BRANCH, ports.BRAND_RD_METRIC_UPDATES_MONTH, ports.BRAND_RD_WORKSPACE_CHOICE])
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it("executes the owner workspace-choice read through the native lane 39 client", async () => {
    const laneEnv = "ERPX_NATIVE_BRAND_LANE39_CONFIG";
    const prior = process.env[laneEnv];
    process.env[laneEnv] = JSON.stringify({
      privateBaseUrl: "http://127.0.0.1:3198",
      timeoutMs: 10000,
      holdings: [
        { origin: "https://acme.jesadakorn.com", host: "acme.jesadakorn.com", holdingId: "h001", holdingSlug: "acme", companyId: "acme-co" },
        { origin: "https://northstar.jesadakorn.com", host: "northstar.jesadakorn.com", holdingId: "h002", holdingSlug: "northstar" },
        { origin: "https://erpx-internal.jesadakorn.com", host: "erpx-internal.jesadakorn.com", holdingId: "h003", holdingSlug: "erpx" },
      ],
    });
    vi.stubGlobal("fetch", vi.fn(async (_url: URL, init: RequestInit) => {
      expect(_url.toString()).toBe("http://127.0.0.1:3198/api/v1/holdings/acme/branding/lane39-workspace-choice");
      expect(String(init.body)).toBe(JSON.stringify({ companyIds: ["co-a", "co-b"] }));
      return new Response(JSON.stringify({ brands: [
        { scope_kind: "holding", scope_id: "holding-1", primary_publication_id: "pub-1" },
        { scope_kind: "branch", scope_id: "branch-1", primary_publication_id: null },
      ] }), { status: 200, headers: { "content-type": "application/json" } });
    }));
    try {
      const { workspaceChoiceBrands } = await import("../../../packages/server-brand/src/adapters/workspace-choice-brand");
      expect(await workspaceChoiceBrands({} as never, ["co-a", "co-b"], { holdingId: "h001", holdingSlug: "acme" })).toEqual([
        { scope_kind: "holding", scope_id: "holding-1", primary_publication_id: "pub-1" },
        { scope_kind: "branch", scope_id: "branch-1", primary_publication_id: null },
      ]);
    } finally {
      if (prior === undefined) delete process.env[laneEnv];
      else process.env[laneEnv] = prior;
    }
  });

  it("plans evidence search with the declared predicate families and bind order", async () => {
    const { executiveEvidenceSources } = await import("../../../packages/server-brand/src/adapters/executive-evidence-sources");
    const { BRAND_RD_EVIDENCE_SEARCH } = await import("../../../packages/server-brand/src/adapters/native-brand-read-ports");
    const { db, bound } = fakeDb();
    const input = {
      metric: "brand-profile-updates",
      filters: { from: "2026-01-01", to: "2026-01-31", currency: "THB" },
      query: "north", limit: 25,
    };
    await executiveEvidenceSources(db as never).searchRows0(actor, input as never, { branchName: "North", id: "profile-1" });
    expect(bodies[0]).toMatchObject({ kind: BRAND_RD_EVIDENCE_SEARCH, range: true, query: true, cursor: true });
    expect(bodies[0].values).toEqual([
      "company-1", "branch-1", "branch-1",
      Date.parse("2026-01-01"), Date.parse("2026-01-31") + 86400000,
      "north", "north", "north",
      "North", "North", "profile-1", 26,
    ]);
    expect(bound[0]).toEqual(bodies[0].values);
  });

  it("selects the metric read kind per reachable compound breakdown", async () => {
    const { readExecutiveMetricProjection } = await import("../../../packages/server-brand/src/adapters/executive-metric-projections");
    const ports = await import("../../../packages/server-brand/src/adapters/native-brand-read-ports");
    const { db } = fakeDb();
    const filters = { from: "2026-01-01", to: "2026-01-31", currency: "THB" };
    await readExecutiveMetricProjection(db as never, actor, "brand-profiles", filters, "default");
    await readExecutiveMetricProjection(db as never, actor, "brand-profiles", filters, "branch");
    await readExecutiveMetricProjection(db as never, actor, "brand-profile-updates", filters, "default");
    expect(bodies.map((b) => b.kind)).toEqual([
      ports.BRAND_RD_METRIC_PROFILES_LABEL,
      ports.BRAND_RD_METRIC_PROFILES_BRANCH,
      ports.BRAND_RD_METRIC_UPDATES_MONTH,
    ]);
    expect(bodies[2].values).toEqual([
      "company-1", "branch-1", "branch-1",
      Date.parse("2026-01-01"), Date.parse("2026-01-31") + 86400000,
    ]);
  });

  it("leaves no TypeScript SELECT literal in the three cut adapters", async () => {
    for (const name of ["executive-evidence-sources", "executive-metric-projections", "workspace-choice-brand"]) {
      const src = await readFile(new URL(`../../../packages/server-brand/src/adapters/${name}.ts`, import.meta.url), "utf8");
      expect(src).not.toMatch(/\bSELECT\s/);
    }
  });
});
