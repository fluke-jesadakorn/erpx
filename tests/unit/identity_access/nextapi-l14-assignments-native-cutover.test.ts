import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, test } from "vitest";
import { nativeApiPath } from "../../../scripts/local/native-api-proxy";

const repo = resolve(__dirname, "../../..");
const read = (path: string) => readFileSync(resolve(repo, path), "utf8");

const retiredHandler =
  "packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/assignments/active/route.ts";
const retiredReexports = [
  "recruit",
  "launcher",
  "learn",
  "compat",
  "sales",
  "projects",
  "admin",
  "pms",
  "manufacturing",
  "supply",
  "pos",
  "support",
  "people",
  "finance",
  "service",
  "legal",
  "assets",
].map((app) => `apps/${app}/src/app/api/v1/holdings/[holdingSlug]/assignments/active/route.ts`);

/** Exact host|METHOD|path entries the lead persists in ERPX_DEV_NATIVE_PATHS. */
const host = "acme.jesadakorn.com";
const path = "/api/v1/holdings/{holding}/assignments/active";
const gatewayEntries = [`${host}|GET|${path}`, `${host}|POST|${path}`] as const;

describe("nextapi L14 assignments/active native cutover", () => {
  test("the Next route handler and every app re-export are gone", () => {
    expect(existsSync(resolve(repo, retiredHandler)), retiredHandler).toBe(false);
    expect(retiredReexports).toHaveLength(17);
    for (const reexport of retiredReexports)
      expect(existsSync(resolve(repo, reexport)), reexport).toBe(false);
  });

  test("both native owners register the exact path and the owner read executes SQL", () => {
    const composition = read("src/composition/src/native_work_roles_http.rs");
    const owner = read("packages/server-identity_access/src/adapters/native_lane_44/http.rs");
    expect(composition).toContain(
      '.route(\n                "/api/v1/holdings/{holding_slug}/assignments/active",\n                get(read_active::<C>).post(select_active::<C>),',
    );
    expect(owner).toContain(
      '.route(\n            "/api/v1/holdings/{holding_slug}/assignments/active",\n            get(assignments_active).post(assignments_active),',
    );
    // The exact public route is owned by the composition router and is still
    // profile-gated; the lead must mount it in every profile.
    expect(composition).toContain("native_work_roles::execute(");
    expect(read("packages/server-identity_access/src/adapters/native_lane_44/assignments_active.rs"))
      .toContain("read_account_work_roles(c, scope, now, true)");
    expect(read("packages/server-identity_access/src/adapters/account_work_roles.rs"))
      .toContain('sqlx::query("UPDATE active_work_contexts SET assignment_id=$1');
    expect(read("src/composition/src/private_development_api.rs")).toContain(
      "crate::native_work_roles_http::native_work_roles_router_with_selection(",
    );
  });

  test("the native payload, validation and cookie envelope match the retired route", () => {
    const active = read("packages/server-identity_access/src/adapters/native_lane_44/assignments_active.rs");
    for (const field of [
      '"valid": selected.is_some()',
      '"revision": view.selection.as_ref().map_or(0, |s| s.revision)',
      '"assignmentVersion": selected.and_then(|s| s.assignment_version)',
      '"companyId": view.company_id',
      '"userId": view.user_id',
      "applies: assignment_applies(a, workspace)",
      "s.expires_at > now",
    ])
      expect(active, field).toContain(field);
    // Zod-equivalent body bounds live in the owner, never in the removed route.
    const composition = read("src/composition/src/native_work_roles_http.rs");
    expect(composition).toContain("if assignment.len() > 128");
    expect(composition).toContain(".filter(|n| (0..9_007_199_254_740_991).contains(n))");
    expect(composition).toContain(
      'for prefix in ["erpx_assignment_", "erpx_role_preference_"]',
    );
    expect(composition).toContain('erpx_company_');
    const http = read("packages/server-identity_access/src/adapters/native_lane_44/http.rs");
    expect(http).toContain('json!({"data": reply.data})');
    expect(http).toContain('(header::CACHE_CONTROL, "private, no-store")');
    expect(http).toContain("header::SET_COOKIE");
  });

  test("the client already bypasses work-context headers for the native paths", () => {
    const fetch = read("packages/feature-work_projection/src/lib/work-fetch.ts");
    expect(fetch).toContain("url.pathname.endsWith('/assignments/active')");
    expect(fetch).toContain("return holdingFetch(input,init)");
  });

  test("exact gateway entries match nativeApiPath and do not over-match", () => {
    const environment = {
      NODE_ENV: "test" as const,
      ERPX_DEV_NATIVE_PATHS: gatewayEntries.join(","),
    };
    for (const entry of gatewayEntries) {
      const [entryHost, method, template] = entry.split("|");
      const concrete = "/api/v1/holdings/acme/assignments/active";
      expect(nativeApiPath(environment, concrete, entryHost, method), entry).toBe(true);
      expect(nativeApiPath(environment, concrete, "other.example.test", method), entry).toBe(false);
      expect(nativeApiPath(environment, concrete, entryHost, "PUT"), entry).toBe(false);
      expect(nativeApiPath(environment, `${concrete}/extra`, entryHost, method), entry).toBe(false);
    }
  });
});
