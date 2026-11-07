import { describe, expect, it, vi } from "vitest";
import { appsMemoryKey, buildAppsCompanies, emptyAppsMemory, readAppsMemory, rememberAppsLaunch, restoreAppsSelection, selectAppsCompany, visibleApps, writeAppsMemory, type AppsEntry, type AppsMemory } from "@erpx/feature-work_projection/lib/apps-launcher";
import type { HoldingWorkspaceScope } from "@erpx/feature-work_projection/lib/workspace";

const companyScope: HoldingWorkspaceScope = { kind: "holding", holdingSlug: "group" };
const branchScope: HoldingWorkspaceScope = { kind: "branch", holdingSlug: "group", branchSlug: "bangkok" };
const departmentScope: HoldingWorkspaceScope = { kind: "department", holdingSlug: "group", branchSlug: "bangkok", departmentSlug: "accounts" };
function entry(companyId = "c1", scope = companyScope, productId = "apps") {
  return { companyId, company: companyId === "c1" ? "Company One" : "Company Two", holding: "Group", workspace: scope.kind === "holding" ? "Company" : scope.kind === "branch" ? "Bangkok / Operations" : "Bangkok / Operations / Accounts", scope,
    ...(scope.kind === "department" ? { branch: "Bangkok / Operations" } : {}), productId, label: productId, href: `https://${productId}.example/sso/start?companyId=${companyId}&area=${scope.kind}` };
}
const products: AppsEntry[] = Array.from({ length: 15 }, (_, index) => ({ productId: `app${index}`, label: `App ${index}`, href: `https://app${index}.example/sso/start` }));
function storage() { const values = new Map<string, string>(); return { values, getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } }; }
const companies = () => buildAppsCompanies([entry(), entry("c1", branchScope), entry("c1", departmentScope), entry("c2")], []);

describe("Apps company and workspace projection", () => {
  it("shows each company once and each app once in its exact workspace, retaining native links and badges", () => {
    const original = { ...entry(), accessLabel: "History only" };
    const result = buildAppsCompanies([original, { ...original, href: "duplicate" }, entry("c1", branchScope), entry("c2")], []);
    expect(result).toHaveLength(2);
    expect(result[0].workspaces).toHaveLength(2);
    expect(result[0].workspaces[0].apps).toEqual([{ productId: "apps", label: "apps", href: original.href, accessLabel: "History only" }]);
    expect(result[1].workspaces[0].apps[0].href).toContain("companyId=c2");
  });
  it("matches Company administration by company and scope, including administration-only companies", () => {
    const result = buildAppsCompanies([entry(), entry("c1", departmentScope)], [
      { ...entry("c1", departmentScope), kind: "company", href: "department-admin" },
      { ...entry("c2"), kind: "company", href: "company-two-admin" },
      { ...entry("c1"), kind: "management", href: "provider-management" },
    ]);
    expect(result[0].workspaces[0].adminHref).toBeUndefined();
    expect(result[0].workspaces[1].adminHref).toBe("department-admin");
    expect(result[1].workspaces[0]).toMatchObject({ apps: [], adminHref: "company-two-admin" });
    expect(result[0].workspaces[0].managementHref).toBe("provider-management");
  });
  it("retains department-only rights without inventing a selectable parent and keeps branch labels containing slashes", () => {
    const [company] = buildAppsCompanies([entry("c1", departmentScope)], []);
    expect(company.workspaces).toHaveLength(1);
    expect(company.workspaces[0]).toMatchObject({ kind: "department", branchLabel: "Bangkok / Operations", branchId: "/holdings/group/branches/bangkok" });
    expect(restoreAppsSelection([company], emptyAppsMemory()).memory.workspaceId).toBe(company.workspaces[0].id);
  });
  it("separates identical company IDs under different holdings", () => {
    const result = buildAppsCompanies([entry(), entry("c1", { kind: "holding", holdingSlug: "other" })], []);
    expect(result).toHaveLength(2); expect(result[0].id).not.toBe(result[1].id);
  });
  it("handles no available entries", () => {
    expect(buildAppsCompanies([], [])).toEqual([]);
    expect(restoreAppsSelection([], emptyAppsMemory()).memory).toEqual(emptyAppsMemory());
  });
});

describe("Apps selection and recent apps", () => {
  it("shows all apps, puts recent entries first and excludes history from other scopes", () => {
    expect(visibleApps(products, ["app14", "unavailable", "app9"], "").map(app => app.productId)).toEqual(["app14", "app9", ...products.filter(app => !["app14", "app9"].includes(app.productId)).map(app => app.productId)]);
    expect(visibleApps(products, [], "")).toEqual(products);
  });
  it("searches every available app without an expansion step", () => {
    expect(visibleApps(products, [], "APP 14")).toEqual([products[14]]);
    expect(visibleApps(products, [], "")).toHaveLength(15);
    expect(visibleApps(products, [], "  ")).toHaveLength(15);
    expect(visibleApps(products, [], "missing")).toEqual([]);
  });
  it("remembers recent apps per company, once each, without storing their SSO links", () => {
    const [one, two] = companies();
    let memory = rememberAppsLaunch(emptyAppsMemory(), one.id, "finance");
    memory = rememberAppsLaunch(memory, two.id, "sales");
    memory = rememberAppsLaunch(memory, one.id, "apps");
    memory = rememberAppsLaunch(memory, one.id, "finance");
    expect(memory.recent).toEqual({ [one.id]: ["finance", "apps"], [two.id]: ["sales"] });
    expect(JSON.stringify(memory)).not.toMatch(/https|sso|href/);
  });
  it("keeps the current choice when editing, but clears the old workspace when selecting another company", () => {
    const all = companies(), [one, two] = all;
    const memory: AppsMemory = { ...emptyAppsMemory(), companyId: one.id, workspaceId: one.workspaces[2].id };
    expect(selectAppsCompany(all, memory, one.id)).toEqual(memory);
    const next = selectAppsCompany(all, memory, two.id);
    expect(next.workspaceId).toBe(two.workspaces[0].id);
    expect(restoreAppsSelection(all, next).memory.companyId).toBe(two.id);
    const other = { ...two, workspaces: [...two.workspaces, { ...two.workspaces[0], id: "other-area" }] };
    expect(selectAppsCompany([one, other], memory, other.id).workspaceId).toBeNull();
  });
  it("restores valid choices and removes revoked app history using the latest authorized snapshot", () => {
    const all = companies(), one = all[0];
    const memory: AppsMemory = { ...emptyAppsMemory(), companyId: one.id, workspaceId: one.workspaces[2].id, recent: { [one.id]: ["revoked", "apps"] } };
    const result = restoreAppsSelection(all, memory);
    expect(result.invalid).toBeNull(); expect(result.memory.workspaceId).toBe(memory.workspaceId);
    expect(result.memory.recent[one.id]).toEqual(["apps"]);
  });
  it("returns to the missing step with a notice when company or workspace authority was revoked", () => {
    const all = companies(), one = all[0];
    const memory: AppsMemory = { ...emptyAppsMemory(), companyId: one.id, workspaceId: "revoked" };
    expect(restoreAppsSelection(all, memory)).toMatchObject({ invalid: "workspace", memory: { companyId: one.id, workspaceId: null } });
    const result = restoreAppsSelection([all[1], { ...all[1], id: "third" }], memory);
    expect(result).toMatchObject({ invalid: "company", memory: { companyId: null, workspaceId: null } });
    expect(JSON.stringify(result)).not.toContain("revoked");
  });
  it("selects a sole company and workspace without opening an app", () => {
    const all = buildAppsCompanies([entry()], []), result = restoreAppsSelection(all, emptyAppsMemory());
    expect(result.memory).toMatchObject({ companyId: all[0].id, workspaceId: all[0].workspaces[0].id, recent: { [all[0].id]: [] } });
  });
});

describe("Optional account-separated browser memory", () => {
  it("restores after refresh and keeps two accounts separate", () => {
    const store = storage(), [one, two] = companies();
    writeAppsMemory(store, "account-a", { ...emptyAppsMemory(), companyId: one.id, workspaceId: one.workspaces[1].id });
    writeAppsMemory(store, "account-b", { ...emptyAppsMemory(), companyId: two.id, workspaceId: two.workspaces[0].id });
    expect(readAppsMemory(store, "account-a").workspaceId).toBe(one.workspaces[1].id);
    expect(readAppsMemory(store, "account-b").companyId).toBe(two.id);
    expect(readAppsMemory(store, "account-c")).toEqual(emptyAppsMemory());
  });
  it.each(["null", "{", "[]", '{"version":2}'])("ignores malformed or incompatible stored data: %s", raw => {
    const store = storage(); store.values.set(appsMemoryKey("a"), raw);
    expect(readAppsMemory(store, "a")).toEqual(emptyAppsMemory());
  });
  it("reads identifiers only and bounds untrusted history", () => {
    const store = storage(); store.values.set(appsMemoryKey("a"), JSON.stringify({ version: 1, companyId: "company", workspaceId: "workspace", href: "private-sso", recent: { company: ["apps", "apps", 7, null, "x".repeat(101)] } }));
    expect(readAppsMemory(store, "a")).toEqual({ version: 1, companyId: "company", workspaceId: "workspace", recent: { company: ["apps"] } });
  });
  it("continues when storage reads or writes are denied or full", () => {
    const blocked = { getItem: () => { throw Error("denied"); }, setItem: () => { throw Error("quota"); } };
    expect(readAppsMemory(blocked, "a")).toEqual(emptyAppsMemory());
    expect(() => writeAppsMemory(blocked, "a", emptyAppsMemory())).not.toThrow();
  });
});

const preferences = vi.hoisted(() => ({ workspace: vi.fn(), company: vi.fn(), bindings: vi.fn() }));
vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-runtime/public/runtime', () => ({ runtimeEnv: async () => ({ CONTROL_DB: 'control' }), activeHoldingBindings: preferences.bindings }));
vi.mock('@erpx/server-work_projection/public/workspace-preferences', () => ({ readWorkspacePreference: preferences.workspace, readCompanyPreference: preferences.company }));

describe('Central account workspace memory across product hosts', () => {
  it('reads Company and exact Department for the same account without exposing authority or SSO links', async () => {
    const { savedAppsWorkspaceForPresentation } = await import('@erpx/server-work_projection/public/workspace-entry-presentation-query');
    preferences.workspace.mockResolvedValue({ workspace_path: '/holdings/group/branches/bangkok/departments/accounts/home' });
    preferences.bindings.mockResolvedValue([{ id: 'h1', slug: 'group' }]);
    preferences.company.mockResolvedValue({ company_id: 'c1' });
    expect(await savedAppsWorkspaceForPresentation('account-a')).toEqual({ companyId: JSON.stringify(['group','c1']), workspaceId: '/holdings/group/branches/bangkok/departments/accounts' });
    expect(preferences.workspace).toHaveBeenLastCalledWith('control', 'account-a');
    expect(preferences.company).toHaveBeenLastCalledWith('control', 'account-a', 'h1');
    preferences.workspace.mockResolvedValue(null);
    expect(await savedAppsWorkspaceForPresentation('account-b')).toBeNull();
    expect(preferences.workspace).toHaveBeenLastCalledWith('control', 'account-b');
  });
  it('treats a removed Holding or Company as an invalid hint to be checked against current choices', async () => {
    const { savedAppsWorkspaceForPresentation } = await import('@erpx/server-work_projection/public/workspace-entry-presentation-query');
    preferences.workspace.mockResolvedValue({ workspace_path: '/holdings/removed/home' });
    preferences.bindings.mockResolvedValue([]);
    const hint = await savedAppsWorkspaceForPresentation('account-a');
    const result = restoreAppsSelection(companies(), { ...emptyAppsMemory(), ...hint });
    expect(result.invalid).toBe('company');
    expect(result.memory.companyId).toBeNull();
  });
});
