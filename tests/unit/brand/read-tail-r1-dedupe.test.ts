import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
vi.mock("server-only", () => ({}));
import { LANE39_CONFIG_ENV } from "../../../packages/server-brand/src/adapters/native-lane-39/client";

/**
 * G4 / retire.brand.read-tail-r2: the two Brand management authority guards were
 * owner-planned native reads. r2 folded their old TypeScript statement heads
 * into a Rust plan oracle; lane 40 replaces that intermediary with owner-
 * executed sqlx statements behind two axum routes. The TypeScript adapter now
 * emits no SQL, no plan request and no bind array; it fails closed on every
 * transport or row-shape failure. This suite pins the switched consumer, the
 * exact axum targets and the owner KPI.
 */
const root = resolve(__dirname, "../../../");
const srcRoot = resolve(root, "packages/server-brand/src");
const adapters = resolve(srcRoot, "adapters");
const read = (name: string) => readFileSync(resolve(adapters, name), "utf8");
const count = (s: string, needle: string) => s.split(needle).length - 1;

const envKey = LANE39_CONFIG_ENV;
const original = process.env[envKey];
const requests: { url: URL; init: RequestInit; body: { id: string } }[] = [];

const configure = () => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: "http://127.0.0.1:44997",
    timeoutMs: 1000,
    holdings: [{
      origin: "https://acme.example.com",
      host: "acme.example.com",
      holdingSlug: "acme",
      holdingId: "h001",
      companyId: "company-1",
    }],
  });
};
const actor = {
  holdingId: "h001", holdingSlug: "acme", companyId: "company-1", branchId: "branch-1",
  assignmentId: "assignment-1", scope: { kind: "branch", branchSlug: "north" },
} as never;
/** The caller unit is retained only for signature compatibility; lane 40 runs
 * the SQL on the owner's registered pool, so no TypeScript db stub is used. */
const fakeDb = () => ({} as never);

describe("Brand management guard native cutover (lane 40)", () => {
  beforeEach(() => {
    requests.length = 0;
    configure();
    vi.stubGlobal("fetch", vi.fn(async (url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { id: string };
      requests.push({ url, init, body });
      const json = (value: unknown) => new Response(JSON.stringify(value), {
        status: 200, headers: { "content-type": "application/json" },
      });
      const backend = process.env["BRAND_TEST_BACKEND"];
      if (backend === "error")
        return new Response(JSON.stringify({ error: { code: "BRAND_MANAGEMENT_READ_FAILED", message: "rejected" } }), {
          status: 400, headers: { "content-type": "application/json" },
        });
      if (backend === "drift") return json({ branch: { id: "br-1" } });
      const company = url.pathname.endsWith("company");
      return json(company
        ? { exists: body.id === "co-1" }
        : { branch: body.id === "br-1" ? { id: "br-1", display_name: "North" } : null });
    }));
  });
  afterEach(() => {
    if (original === undefined) delete process.env[envKey];
    else process.env[envKey] = original;
    delete process.env["BRAND_TEST_BACKEND"];
    vi.unstubAllGlobals();
  });

  test("both guards execute on the owner axum route with no TypeScript statement", async () => {
    const ports = await import("../../../packages/server-brand/src/adapters/native-brand-management");
    const db = fakeDb();
    expect(await ports.nativeBrandManagementBranch(db, actor, "br-1"))
      .toEqual({ id: "br-1", display_name: "North" });
    expect(requests[0]!.url.toString()).toBe(
      "http://127.0.0.1:44997/api/v1/holdings/acme/branding/lane40-management-branch",
    );
    expect(requests[0]!.body).toEqual({ id: "br-1" });
    expect(String(requests[0]!.body)).not.toContain("SELECT");
    expect((requests[0]!.init.headers as Headers).get("x-erpx-context-company")).toBe("company-1");

    expect(await ports.nativeBrandManagementCompanyExists(db, actor, "co-1")).toBe(true);
    expect(requests[1]!.url.toString()).toBe(
      "http://127.0.0.1:44997/api/v1/holdings/acme/branding/lane40-management-company",
    );
    expect(requests[1]!.body).toEqual({ id: "co-1" });
    expect(await ports.nativeBrandManagementCompanyExists(db, actor, "co-2")).toBe(false);
    expect(await ports.nativeBrandManagementBranch(db, actor, "br-2")).toBeNull();
  });

  test("native guard failures and malformed rows fail closed without a TS fallback", async () => {
    const ports = await import("../../../packages/server-brand/src/adapters/native-brand-management");
    const db = fakeDb();
    await expect(ports.nativeBrandManagementBranch(db, actor, "bad/id"))
      .rejects.toMatchObject({ code: "BRAND_MANAGEMENT_REQUEST_INVALID", status: 422 });
    expect(requests).toHaveLength(0);

    process.env["BRAND_TEST_BACKEND"] = "drift";
    await expect(ports.nativeBrandManagementBranch(db, actor, "br-1"))
      .rejects.toMatchObject({ code: "BRAND_MANAGEMENT_RESPONSE_INVALID", status: 502 });
    delete process.env["BRAND_TEST_BACKEND"];

    delete process.env[envKey];
    await expect(ports.nativeBrandManagementBranch(db, actor, "br-1"))
      .rejects.toMatchObject({ code: "BRAND_MANAGEMENT_CONFIGURATION_REQUIRED" });

    configure();
    process.env["BRAND_TEST_BACKEND"] = "error";
    await expect(ports.nativeBrandManagementCompanyExists(db, actor, "co-1"))
      .rejects.toMatchObject({ code: "BRAND_MANAGEMENT_READ_FAILED", status: 400 });
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
    // The plan-only intermediary for this family is deleted; SQL now executes in Rust.
    expect(read("native_read_sql.rs")).not.toContain("RD_MANAGEMENT_BRANCH_EXISTS");
    expect(read("native_read_sql.rs")).not.toContain("RD_MANAGEMENT_COMPANY_EXISTS");
    const lane = readFileSync(resolve(adapters, "native_lane_40/http.rs"), "utf8");
    expect(lane).toContain("lane40-management-branch");
    expect(lane).toContain("lane40-management-company");
  });

  test("the cross-owner scope probe renders from the Rust-owned scope-head contract", () => {
    const probe = read("attachment-read.ts");
    expect(count(probe, "SELECT ")).toBe(0);
    expect(probe).toContain("brandAttachmentScopeProbeSql(currentScope.where)");
    expect(probe).toContain(".bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args)");
    expect(probe).toContain("lockQueuedCommandAuthority(database)");
    const helper = read("attachment-scope-head.ts");
    expect(count(helper, "SELECT ")).toBe(0);
    expect(helper).toContain('import contract from "./attachment-scope-head-sources.json" with { type: "json" }');
    expect(helper).toContain("heads.scopeHead");
    const contract = JSON.parse(readFileSync(resolve(adapters, "attachment-scope-head-sources.json"), "utf8")) as {
      scopeHead: string;
    };
    expect(contract.scopeHead).toBe(
      "SELECT 1 FROM(VALUES(?::text,?::text,?::text))workforce_actor_scope(company_id,branch_id,department_id) WHERE ",
    );
  });

  test("every live presentation consumer delegates to the mounted native owner", () => {
    const branding = read("branding.ts");
    expect(branding).not.toMatch(/\bSELECT\s/);
    expect(branding).not.toContain("db.prepare");
    expect(branding).toContain("nativeResolveWorkspaceBrand(a)");
    expect(branding).toContain("nativeResolveDocumentBrand(a,scope)");
    expect(branding).toContain("nativeBrandProfileEvidence(a,branchId)");
    expect(read("business-document.ts")).toContain("nativeBrandRecordBranch");
    expect(read("identity.ts")).toContain("nativeBrandIdentityWorkspace");
    const router = readFileSync(resolve(srcRoot, "adapters/branding_http.rs"), "utf8");
    for (const kind of ["workspace", "document", "profiles", "storefront", "evidence", "record-branch"])
      expect(router).toContain(`"${kind}"`);
    const composition = readFileSync(
      resolve(root, "src/composition/src/private_development_api.rs"),
      "utf8",
    );
    expect(composition).toContain("branding_http_router(branding_targets)");
  });

  test("storefront front-projection is the last Brand SQL fragment and needs an Organization cut", () => {
    const fragment = read("front-brand-projection.ts");
    expect(count(fragment, "SELECT ")).toBe(0);
    expect(fragment).toContain(
      'storefrontBrandColumns="b.short_name,b.accent_color,b.logo_attachment_id,b.logo_publication_id"',
    );
    expect(fragment).toContain('storefrontBrandJoin="LEFT JOIN storefront_app_brands b ON b.company_id=c.id"');
    const consumer = readFileSync(
      resolve(root, "packages/server-organization/src/adapters/front-company-facts.ts"),
      "utf8",
    );
    expect(consumer).toContain("@erpx/server-brand/public/front-brand-projection");
    const publicBranding = readFileSync(resolve(srcRoot, "public/branding.ts"), "utf8");
    expect(publicBranding).toContain("nativeReadStorefrontAppBrand");
    // The three native consumer switches stay explicit; an unset switch fails
    // closed rather than falling back to a TypeScript statement.
    expect(read("native-brand-presentation.ts")).toContain(
      'const CONFIG_ENV = "ERPX_NATIVE_BRANDING_CONFIG"',
    );
    expect(read("native-brand-read-ports.ts")).toContain(
      'const ENV = "ERPX_NATIVE_BRAND_READ_CONFIG"',
    );
    expect(read("native-lane-39/client.ts")).toContain(
      'LANE39_CONFIG_ENV = "ERPX_NATIVE_BRAND_LANE39_CONFIG"',
    );
  });

  test("retirement:ts-statements drops 1 -> 0 for the owner", () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith(".ts")) total += count(readFileSync(path, "utf8"), "SELECT ");
      }
    };
    walk(srcRoot);
    // r1/r2 removed the TypeScript guard literals; lane 40 keeps the owner at 0.
    expect(total).toBe(0);
    expect(total).toBeLessThan(1);
    expect(count(read("branding.ts"), "SELECT ")).toBe(0);
    expect(count(read("attachment-read.ts"), "SELECT ")).toBe(0);
    expect(count(read("attachment-scope-head.ts"), "SELECT ")).toBe(0);
  });
});
