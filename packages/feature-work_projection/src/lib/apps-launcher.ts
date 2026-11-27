import { workspaceBase, type HoldingWorkspaceScope } from "@erpx/feature-work_projection/lib/workspace";

export type AppsEntry = { productId: string; label: string; href: string; accessLabel?: string | null };
type WorkspaceEntry = { companyId: string; company: string; holding: string; workspace: string; branch?: string; scope: HoldingWorkspaceScope };
export type AppsWorkspace = {
  id: string; label: string; kind: HoldingWorkspaceScope["kind"];
  branchId?: string; branchLabel?: string; apps: AppsEntry[]; adminHref?: string; managementHref?: string;
};
export type AppsCompany = { id: string; label: string; holding: string; holdingSlug?: string; workspaces: AppsWorkspace[] };

/** Input consists only of current owner-authorized launch links. Never infer parent access. */
export function buildAppsCompanies(products: readonly (WorkspaceEntry & AppsEntry)[], admins: readonly (WorkspaceEntry & { kind: string; href: string })[]): AppsCompany[] {
  const companies = new Map<string, AppsCompany>();
  function workspace(entry: WorkspaceEntry) {
    const companyId = JSON.stringify([entry.scope.holdingSlug, entry.companyId]);
    let company = companies.get(companyId);
    if (!company) { company = { id: companyId, label: entry.company, holding: entry.holding, holdingSlug: entry.scope.holdingSlug, workspaces: [] }; companies.set(companyId, company); }
    const id = workspaceBase(entry.scope);
    let result = company.workspaces.find(item => item.id === id);
    if (!result) {
      const branchId = "branchSlug" in entry.scope && entry.scope.branchSlug
        ? workspaceBase({ kind: "branch", holdingSlug: entry.scope.holdingSlug, branchSlug: entry.scope.branchSlug }) : undefined;
      const branchLabel = entry.branch ?? (entry.scope.kind === "branch" ? entry.workspace : undefined);
      result = { id, label: entry.workspace, kind: entry.scope.kind, branchId, branchLabel, apps: [] };
      company.workspaces.push(result);
    }
    return result;
  }
  for (const entry of products) {
    const target = workspace(entry);
    if (!target.apps.some(app => app.productId === entry.productId)) target.apps.push({ productId: entry.productId, label: entry.label, href: entry.href, accessLabel: entry.accessLabel });
  }
  for (const entry of admins) {
    if (entry.kind === "company") workspace(entry).adminHref = entry.href;
    if (entry.kind === "management") workspace(entry).managementHref = entry.href;
  }
  return [...companies.values()];
}

export type AppsMemory = { version: 1; companyId: string | null; workspaceId: string | null; recent: Record<string, string[]> };
export const emptyAppsMemory = (): AppsMemory => ({ version: 1, companyId: null, workspaceId: null, recent: {} });
export const appsMemoryKey = (accountId: string) => `erpx:apps-launcher:v1:${encodeURIComponent(accountId)}`;

export function feedEntryHref(companies: readonly AppsCompany[], saved: Pick<AppsMemory, "companyId" | "workspaceId"> | null) {
  if (saved?.companyId || saved?.workspaceId) {
    return companies.find(company => company.id === saved.companyId)?.workspaces
      .find(workspace => workspace.id === saved.workspaceId)?.apps.find(app => app.productId === "apps")?.href;
  }
  const [company] = companies;
  if (companies.length !== 1 || company.workspaces.length !== 1) return undefined;
  return company.workspaces[0].apps.find(app => app.productId === "apps")?.href;
}

export function readAppsMemory(storage: Pick<Storage, "getItem">, accountId: string): AppsMemory {
  try {
    const value = JSON.parse(storage.getItem(appsMemoryKey(accountId)) ?? "null");
    if (!value || value.version !== 1) return emptyAppsMemory();
    const recent: Record<string, string[]> = {};
    if (value.recent && typeof value.recent === "object" && !Array.isArray(value.recent)) {
      for (const [id, entries] of Object.entries(value.recent).slice(0, 100)) {
        if (Array.isArray(entries)) recent[id] = [...new Set(entries.filter((entry): entry is string => typeof entry === "string" && entry.length < 100))].slice(0, 30);
      }
    }
    return { version: 1, companyId: typeof value.companyId === "string" ? value.companyId : null, workspaceId: typeof value.workspaceId === "string" ? value.workspaceId : null, recent };
  } catch { return emptyAppsMemory(); }
}

/** Persist identifiers only, including when an untrusted stored object had extra fields. */
export function writeAppsMemory(storage: Pick<Storage, "setItem">, accountId: string, memory: AppsMemory) {
  try { storage.setItem(appsMemoryKey(accountId), JSON.stringify({ version: 1, companyId: memory.companyId, workspaceId: memory.workspaceId, recent: memory.recent })); }
  catch { /* Browsing and SSO must also work with unavailable or full storage. */ }
}

export function restoreAppsSelection(companies: readonly AppsCompany[], memory: AppsMemory) {
  const rememberedCompany = companies.find(company => company.id === memory.companyId);
  const company = rememberedCompany ?? (companies.length === 1 ? companies[0] : undefined);
  const rememberedWorkspace = rememberedCompany?.workspaces.find(workspace => workspace.id === memory.workspaceId);
  const workspace = rememberedWorkspace ?? (company?.workspaces.length === 1 ? company.workspaces[0] : undefined);
  const invalid = memory.companyId && !rememberedCompany ? "company" : memory.workspaceId && rememberedCompany && !rememberedWorkspace ? "workspace" : null;
  const recent = Object.fromEntries(companies.map(item => {
    const available = new Set(item.workspaces.flatMap(area => area.apps.map(app => app.productId)));
    return [item.id, (memory.recent[item.id] ?? []).filter(id => available.has(id))];
  }));
  return { memory: { version: 1 as const, companyId: company?.id ?? null, workspaceId: workspace?.id ?? null, recent }, invalid };
}

export function selectAppsCompany(companies: readonly AppsCompany[], memory: AppsMemory, companyId: string): AppsMemory {
  const company = companies.find(item => item.id === companyId);
  if (!company) return memory;
  return { ...memory, companyId, workspaceId: companyId === memory.companyId && company.workspaces.some(item => item.id === memory.workspaceId)
    ? memory.workspaceId : company.workspaces.length === 1 ? company.workspaces[0].id : null };
}

export function visibleApps(apps: readonly AppsEntry[], query: string) {
  const search = query.trim().toLocaleLowerCase();
  const matches = search ? apps.filter(app => `${app.label} ${app.productId}`.toLocaleLowerCase().includes(search)) : apps;
  const feed = matches.find(app => app.productId === "apps");
  return feed ? [feed, ...matches.filter(app => app !== feed)] : matches;
}

export function recentApps(apps: readonly AppsEntry[], recent: readonly string[], currentId?: string) {
  const byId = new Map(apps.map(app => [app.productId, app]));
  const ids = new Set(currentId ? [currentId, "apps", ...recent] : ["apps", ...recent]);
  return [...ids].flatMap(id => byId.get(id) ?? []).slice(0, 4);
}

export function rememberAppsLaunch(memory: AppsMemory, companyId: string, productId: string): AppsMemory {
  return { ...memory, recent: { ...memory.recent, [companyId]: [productId, ...(memory.recent[companyId] ?? []).filter(id => id !== productId)].slice(0, 30) } };
}

export type FeedScrollScope = {
  accountId: string; companyId: string; workspaceId: string; route: string;
};
export type FeedScrollPosition = { x: number; y: number };
const feedScrollKey = (scope: FeedScrollScope) =>
  `erpx:apps-feed-scroll:v1:${JSON.stringify([scope.accountId, scope.companyId, scope.workspaceId, scope.route])}`;

export function saveFeedScroll(
  storage: Pick<Storage, "setItem">, scope: FeedScrollScope, position: FeedScrollPosition,
) {
  storage.setItem(feedScrollKey(scope), JSON.stringify(position));
}

export function takeFeedScroll(
  storage: Pick<Storage, "getItem" | "removeItem">, scope: FeedScrollScope,
): FeedScrollPosition | null {
  const key = feedScrollKey(scope);
  const raw = storage.getItem(key);
  if (raw === null) return null;
  storage.removeItem(key);
  try {
    const position = JSON.parse(raw) as FeedScrollPosition | null;
    if (position && Number.isFinite(position.x) && Number.isFinite(position.y) && position.y >= 0)
      return { x: position.x, y: position.y };
  } catch { /* Discard corrupted position data. */ }
  return null;
}
