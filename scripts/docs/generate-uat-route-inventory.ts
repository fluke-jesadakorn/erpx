import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { relative, resolve } from "node:path";

const root = resolve(import.meta.dir, "../..");
const appRoot = resolve(root, "src/app");
const outputRoot = resolve(root, "docs/testing");
const catalogueRoot = resolve(root, "tests/catalogue");
const check = process.argv.includes("--check");

function walk(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}

const pageFiles = walk(appRoot)
  .filter((path) => /\/page\.(?:tsx|ts|jsx|js)$/.test(path) && !relative(appRoot,path).split("/").some(segment=>segment.startsWith("_")||segment.startsWith("@")))
  .sort();
const publicPrefixes = [
  "/access",
  "/activate",
  "/book/",
  "/forms/",
  "/dev/",
  "/forgot-password",
  "/front-access",
  "/join",
  "/legal-portal/enroll",
  "/login",
  "/marketing",
  "/reset-password",
  "/signup",
  "/verify-email",
];
const stateSignals: Array<[string, RegExp]> = [
  ["loading", /loading|pending|busy|skeleton/i],
  ["empty", /empty|no results|no records/i],
  ["error", /error|failed|unable/i],
  ["permission", /permission|unauthorized|forbidden|access denied/i],
  ["read-only", /read.?only|canEdit/i],
  ["destructive", /delete|remove|revoke|cancel/i],
  ["success", /success|saved|complete/i],
  ["responsive", /mobile|responsive|viewport/i],
  ["dark", /dark|theme/i],
  ["long/Thai", /[\u0E00-\u0E7F]|overflow-wrap|text-overflow/i],
];

function routeTemplate(path: string): string {
  const route = relative(appRoot, path).replace(/(^|\/)page\.(?:tsx|ts|jsx|js)$/, "");
  return route ? `/${route}` : "/";
}

function category(route: string): string {
  if (route.startsWith("/holdings/")) return "Holding workspace";
  if (route.startsWith("/front/")) return "Front PMS/POS";
  if (route.startsWith("/platform/")) return "Platform";
  if (route.startsWith("/book/")) return "Public booking";
  if (route.startsWith("/forms/")) return "Public forms";
  if (route.startsWith("/customer-portal/")) return "Customer portal";
  if (route.startsWith("/legal-portal/")) return "Legal portal";
  return "Public and account";
}

function primaryActor(route: string): string {
  if (route.startsWith("/forms/")) return "public form visitor";
  if (route.startsWith("/platform/")) return "platform administrator (external identity required)";
  if (route.startsWith("/front/") && route.includes("/[app]/")) return "pms_frontdesk or pos_cashier";
  if (/\/pms(?:\/|$)/.test(route)) return "pms_frontdesk";
  if (/\/pos(?:\/|$)/.test(route)) return "pos_cashier";
  if (/\/finance(?:\/|$)|\/supplier-bills(?:\/|$)|\/stock-counts(?:\/|$)|\/stock-transfers(?:\/|$)/.test(route)) return "finance_approver";
  if (/\/people(?:\/|$)|\/benefits(?:\/|$)|\/talent(?:\/|$)|\/overtime(?:\/|$)/.test(route)) return "people_manager";
  if (/\/support(?:\/|$)/.test(route)) return "support_agent";
  if (/\/procurement-rfqs(?:\/|$)|\/commerce(?:\/|$)/.test(route)) return "purchasing_operator or sales_operator";
  if (/\/work(?:\/|$)|\/leave(?:\/|$)|\/calendar$|\/inbox$|\/home$/.test(route)) return "employee";
  return "holding_owner";
}

function signedOut(route: string): "public" | "login-or-explicit-denial" {
  return publicPrefixes.some((prefix) => route === prefix || route.startsWith(prefix))
    ? "public"
    : "login-or-explicit-denial";
}

const entries = pageFiles.map((path) => {
  const route = routeTemplate(path);
  const source = readFileSync(path, "utf8");
  const holdingRoute = route.startsWith("/holdings/[holdingSlug]");
  const dynamicParameters = [...route.matchAll(/\[\[?\.\.\.([\w]+)\]?\]|\[([\w]+)\]/g)].map((match) => match[1] ?? match[2]);
  return {
    source: relative(root, path),
    template: route,
    area: category(route),
    signedOut: signedOut(route),
    primaryActor: primaryActor(route),
    workspaceScopes: holdingRoute && !/\/(?:pms|pos)(?:\/|$)/.test(route)
      ? ["holding", "branch", "department", "branch-department"]
      : holdingRoute ? ["holding"] : [],
    dynamicParameters: [...new Set(dynamicParameters)],
    detailRoute: route.startsWith("/forms/") || dynamicParameters.some((name) => ["id", "reportId", "propertyId"].includes(name)),
    sourceSignals: stateSignals.filter(([, pattern]) => pattern.test(source)).map(([name]) => name),
  };
});

const sourceHash = createHash("sha256");
for (const path of pageFiles) {
  sourceHash.update(relative(root, path)).update("\0").update(readFileSync(path)).update("\0");
}
const sourceFingerprint = sourceHash.digest("hex");
const inventory = {
  generatedAt: new Date().toISOString(),
  generatedDate: new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Bangkok" }).format(new Date()),
  sourceFingerprint,
  pageRouteCount: entries.length,
  workspaceScopePatterns: ["holding", "branch", "department", "branch-department"],
  viewports: ["desktop Chrome", "Pixel 7 emulation"],
  entries,
};

if (check) {
  const current = JSON.parse(readFileSync(resolve(catalogueRoot, "uat-route-inventory.json"), "utf8")) as typeof inventory;
  if (current.sourceFingerprint !== inventory.sourceFingerprint || current.pageRouteCount !== inventory.pageRouteCount ||
    JSON.stringify(current.entries) !== JSON.stringify(inventory.entries)) throw new Error("UAT_ROUTE_INVENTORY_STALE");
  inventory.generatedAt = current.generatedAt;
  inventory.generatedDate = current.generatedDate;
} else {
  mkdirSync(outputRoot, { recursive: true });
  mkdirSync(catalogueRoot, { recursive: true });
  writeFileSync(resolve(catalogueRoot, "uat-route-inventory.json"), `${JSON.stringify(inventory, null, 2)}\n`);
}
const rows = entries.map((entry) => {
  const details = entry.detailRoute ? "sample record + invalid ID" : "page route";
  const scopes = entry.workspaceScopes.join(", ") || "—";
  return `| \`${entry.template}\` | ${entry.area} | ${entry.signedOut} | ${entry.primaryActor} | ${scopes} | ${details} |`;
});
const markdown = [
  "# ERPX UAT route inventory",
  "",
  `Generated from \`src/app\` page files on ${inventory.generatedDate}. The route source fingerprint is \`${sourceFingerprint}\`. This inventories implemented page files and is not a capability-catalog acceptance claim.`,
  "",
  `- ${entries.length} page routes in source (current source count)`,
  "- Desktop Chrome and Pixel 7 emulation",
  "- Anonymous, intended-role and explicit-denial access checks are recorded separately in Playwright results.",
  "- Holding routes are exercised at holding, branch, department, and branch-plus-department scopes.",
  "- Dynamic detail routes use a simulation fixture ID when one exists and also test an invalid ID.",
  "",
  "| Route template | Area | Signed-out expectation | Primary role | Workspace scopes | Data case |",
  "| --- | --- | --- | --- | --- | --- |",
  ...rows,
  "",
  "## Acceptance boundary",
  "",
  "This inventory covers the page files present in source at the fingerprint above. A route passes only when the expected login/permission/Coming Soon/not-found state is visible, the page has no server error, and mobile content does not overflow horizontally. Existing workflow specs remain the interaction coverage for create, review, validation, reload, and recovery behavior.",
  "",
].join("\n");
if (check) {
  if (readFileSync(resolve(outputRoot, "uat-route-inventory.md"), "utf8") !== markdown) throw new Error("UAT_ROUTE_DOCUMENTATION_STALE");
} else writeFileSync(resolve(outputRoot, "uat-route-inventory.md"), markdown);
console.log(JSON.stringify({ pageRouteCount: entries.length, sourceFingerprint }));
