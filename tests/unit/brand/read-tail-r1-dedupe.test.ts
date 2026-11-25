import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
vi.mock("server-only", () => ({}));

/**
 * G4 / retire.brand.read-tail-r2: the two Brand management authority guards are
 * now owner-planned native reads. r1 folded their old TypeScript statement head
 * into `read-tail-r1.ts`; r2 deletes that file and routes both guards through
 * the owner read port. The exact retired statements are Rust oracles in
 * `native_read_sql.rs`; this suite pins the switched consumer, binds and KPI.
 */
const root = resolve(__dirname, "../../../");
const srcRoot = resolve(root, "packages/server-brand/src");
const adapters = resolve(srcRoot, "adapters");
const read = (name: string) => readFileSync(resolve(adapters, name), "utf8");
const count = (s: string, needle: string) => s.split(needle).length - 1;

const SRC = 'SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes';
const BRANCH_SQL = `SELECT id,display_name FROM (${SRC}) organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'`;
const COMPANY_SQL = `SELECT 1 FROM (${SRC}) organization_nodes WHERE id=? AND kind='company' AND status='active'`;

const envKey = "ERPX_NATIVE_BRAND_READ_CONFIG";
const original = process.env[envKey];
const requests: { url: URL; init: RequestInit; body: { kind: string; values: unknown[] } }[] = [];

const configure = () => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: "http://127.0.0.1:44997",
    timeoutMs: 1000,
    holdings: [{ origin: "https://acme.example.com", holdingSlug: "acme", holdingId: "h001" }],
  });
};
const actor = {
  holdingId: "h001", holdingSlug: "acme", companyId: "company-1", branchId: "branch-1",
  assignmentId: "assignment-1", scope: { kind: "branch", branchSlug: "north" },
} as never;
const fakeDb = (row: Record<string, unknown> | null) => {
  const prepared: string[] = [];
  const bound: unknown[][] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
        async first<T>() { return row as T | null; },
      };
    },
  };
  return { db, prepared, bound };
};

describe("Brand management guard native cutover (r2)", () => {
  beforeEach(() => {
    requests.length = 0;
    configure();
    vi.stubGlobal("fetch", vi.fn(async (url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as typeof requests[number]["body"];
      requests.push({ url, init, body });
      const backend = process.env["BRAND_TEST_BACKEND"];
      if (backend === "error")
        return new Response(JSON.stringify({ error: { code: "BRAND_READ_PLAN_INVALID", message: "rejected" } }), {
          status: 400, headers: { "content-type": "application/json" },
        });
      const sql = body.kind === "brand_rd_management_branch_exists" ? BRANCH_SQL : COMPANY_SQL;
      return new Response(JSON.stringify({ data: { sql, values: body.values } }), {
        status: 200, headers: { "content-type": "application/json" },
      });
    }));
  });
  afterEach(() => {
    if (original === undefined) delete process.env[envKey];
    else process.env[envKey] = original;
    delete process.env["BRAND_TEST_BACKEND"];
    vi.unstubAllGlobals();
  });

  test("both guards execute owner-planned statements with unchanged binds and row shapes", async () => {
    const ports = await import("../../../packages/server-brand/src/adapters/native-brand-management");
    const branchDb = fakeDb({ id: "br-1", display_name: "North" });
    expect(await ports.nativeBrandManagementBranch(branchDb.db as never, actor, "br-1"))
      .toEqual({ id: "br-1", display_name: "North" });
    expect(branchDb.prepared).toEqual([BRANCH_SQL]);
    expect(branchDb.bound).toEqual([["br-1", "company-1"]]);
    expect(requests[0]!.body).toEqual({
      kind: "brand_rd_management_branch_exists", values: ["br-1", "company-1"],
      range: false, query: false, cursor: false,
    });

    const companyDb = fakeDb({ present: 1 });
    expect(await ports.nativeBrandManagementCompanyExists(companyDb.db as never, actor, "co-1")).toBe(true);
    expect(companyDb.prepared).toEqual([COMPANY_SQL]);
    expect(companyDb.bound).toEqual([["co-1"]]);
    expect(requests[1]!.body).toEqual({
      kind: "brand_rd_management_company_exists", values: ["co-1"],
      range: false, query: false, cursor: false,
    });
    expect(await ports.nativeBrandManagementCompanyExists(fakeDb(null).db as never, actor, "co-2")).toBe(false);
    expect(await ports.nativeBrandManagementBranch(fakeDb(null).db as never, actor, "br-2")).toBeNull();
  });

  test("native read failures and malformed rows fail closed without a TS fallback", async () => {
    const ports = await import("../../../packages/server-brand/src/adapters/native-brand-management");
    await expect(ports.nativeBrandManagementBranch(fakeDb(null).db as never, actor, "bad/id"))
      .rejects.toMatchObject({ code: "BRAND_MANAGEMENT_REQUEST_INVALID", status: 422 });
    expect(requests).toHaveLength(0);

    await expect(ports.nativeBrandManagementBranch(fakeDb({ id: "br-1" }).db as never, actor, "br-1"))
      .rejects.toMatchObject({ code: "NATIVE_BRAND_MANAGEMENT_RESPONSE_INVALID", status: 502 });
    await expect(ports.nativeBrandManagementCompanyExists(fakeDb(null).db as never, actor, "co-1"))
      .resolves.toBe(false);

    delete process.env[envKey];
    await expect(ports.nativeBrandManagementBranch(fakeDb(null).db as never, actor, "br-1"))
      .rejects.toMatchObject({ code: "NATIVE_BRAND_READ_CONFIGURATION_REQUIRED" });

    configure();
    process.env["BRAND_TEST_BACKEND"] = "error";
    await expect(ports.nativeBrandManagementCompanyExists(fakeDb(null).db as never, actor, "co-1"))
      .rejects.toMatchObject({ code: "NATIVE_BRAND_READ_FAILED", status: 400 });
  });

  test("the switched consumer and native transports author no statement text", () => {
    const branding = read("branding.ts");
    expect(branding).not.toMatch(/\bSELECT\s/);
    expect(branding).not.toContain("organizationProjectionSources");
    expect(branding).not.toContain("read-tail-r1");
    expect(branding).toContain('from "./native-brand-management"');
    expect(branding).toContain("nativeBrandManagementBranch(db,actor,id)");
    expect(branding).toContain("nativeBrandManagementCompanyExists(db,actor,id)");
    expect(branding).toContain("BRAND_MANAGEMENT_FORBIDDEN");
    expect(existsSync(resolve(adapters, "read-tail-r1.ts"))).toBe(false);
    for (const name of ["native-brand-management.ts", "native-brand-read-ports.ts"])
      expect(read(name)).not.toMatch(/\bSELECT\s/);
  });

  test("the cross-owner scope probe stays as the single excluded residual", () => {
    const probe = read("attachment-read.ts");
    expect(count(probe, "SELECT ")).toBe(1);
    expect(probe).toContain("`SELECT 1 FROM(VALUES(?::text,?::text,?::text))workforce_actor_scope(company_id,branch_id,department_id) WHERE ${currentScope.where}`");
    expect(probe).toContain(".bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args)");
    expect(probe).toContain("lockQueuedCommandAuthority(database)");
  });

  test("retirement:ts-statements drops 2 -> 1 for the owner", () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".ts")) total += count(readFileSync(path, "utf8"), "SELECT ");
      }
    };
    walk(srcRoot);
    // r1: 3 -> 2 (two literals folded into one helper); r2: the helper is
    // deleted and both guards plan on the owner, leaving the excluded probe.
    expect(total).toBe(1);
    expect(total).toBeLessThan(2);
    expect(count(read("branding.ts"), "SELECT ")).toBe(0);
    expect(count(read("attachment-read.ts"), "SELECT ")).toBe(1);
  });
});
