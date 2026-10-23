import { developmentProductOrigin, productIds, sharedHostKeys, type ProductHostKey } from "./product-registry.ts";

export type AppId = ProductHostKey;
export type AppDirectoryId = Exclude<AppId, "apps"> | "launcher";
export type AppDefinition = {
  id: AppId;
  package: `@erpx/app-${AppId}`;
  directory: `apps/${AppDirectoryId}`;
  origin: string;
  port: number;
};

/** Product order is persisted so a workspace addition cannot move existing ports. */
export const appDefinitions: readonly AppDefinition[] = [...productIds, ...sharedHostKeys].map((id, index) => ({
  id, package: `@erpx/app-${id}`, directory: id === "apps" ? "apps/launcher" : `apps/${id}`,
  origin: developmentProductOrigin(id), port: 3201 + index,
}));
export const appIds = appDefinitions.map(app => app.id);
export function appDefinition(id: string): AppDefinition {
  const app = appDefinitions.find(app => app.id === id);
  if (!app) throw new Error(`UNKNOWN_APP:${id}`);
  return app;
}
export function appForHost(host: string): AppDefinition | undefined {
  return appDefinitions.find(app => new URL(app.origin).host === host);
}
/** Filesystem placement is independent of stable host/CLI identities. */
export function appDirectory(id: string): AppDefinition["directory"] | "apps/compat" {
  return id === "compat" ? "apps/compat" : appDefinition(id).directory;
}
