import { developmentProductOrigin, productIds, sharedHostKeys, type ProductHostKey } from "./product-registry.ts";
import {holdingRoutingEnabled, accountDevelopmentOrigin, type OriginProfile} from './holding-hosts.ts';

export type AppId = ProductHostKey;
export type AppDirectoryId = Exclude<AppId, "apps"> | "launcher";
export type AppDefinition = {
  id: AppId;
  package: `@erpx/app-${AppId}`;
  directory: `apps/${AppDirectoryId}`;
  origin: string;
  port: number;
};

/** Explicit assignments preserve every existing listener. Retired Work port 3201 is unused. */
const appPorts: Record<AppId, number> = {
  apps: 3216, pos: 3202, pms: 3203, finance: 3204, people: 3205, sales: 3206,
  supply: 3207, assets: 3208, projects: 3209, recruit: 3210, learn: 3211,
  support: 3212, service: 3213, legal: 3214, manufacturing: 3215,
  account: 3217, admin: 3218, booking: 3219, portal: 3220, "legal-portal": 3221, forms: 3222,
};
export const appDefinitions: readonly AppDefinition[] = [...productIds, ...sharedHostKeys].map(id => ({
  id, package: `@erpx/app-${id}`, directory: id === "apps" ? "apps/launcher" : `apps/${id}`,
  origin: developmentProductOrigin(id), port: appPorts[id],
}));
export const appIds = appDefinitions.map(app => app.id);
export function appDefinition(id: string): AppDefinition {
  const app = appDefinitions.find(app => app.id === id);
  if (!app) throw new Error(`UNKNOWN_APP:${id}`);
  return app;
}
export function appForHost(host: string, env: Record<string, string | undefined> = {}, profile: OriginProfile = 'public'): AppDefinition | undefined {
  if (holdingRoutingEnabled(env)) return host === new URL(accountDevelopmentOrigin(profile)).host ? appDefinitions.find(app => app.id === 'account') : undefined;
  return appDefinitions.find(app => new URL(app.origin).host === host);
}
/** Retired or unknown product names must never reach compatibility fallback. */
export function unregisteredDevelopmentProductHost(host: string | undefined, env: Record<string, string | undefined> = {}, profile: OriginProfile = 'public'): boolean {
  if (holdingRoutingEnabled(env) && ['dev-pos.jesadakorn.com', 'dev-pms.jesadakorn.com'].includes(host ?? '')) return true;
  return !!host && /^[a-z0-9-]+-dev-erpx\.jesadakorn\.com\.?(?::\d+)?$/i.test(host)
    && host !== "console-dev-erpx.jesadakorn.com" && !appForHost(host, env, profile);
}
/** Filesystem placement is independent of stable host/CLI identities. */
export function appDirectory(id: string): AppDefinition["directory"] | "apps/compat" {
  return id === "compat" ? "apps/compat" : appDefinition(id).directory;
}
