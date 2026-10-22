import { developmentProductOrigin, productIds, sharedHostKeys, type ProductHostKey } from "./product-registry.ts";

export type AppId = ProductHostKey;
export type AppDefinition = {
  id: AppId;
  package: `@erpx/app-${AppId}`;
  directory: `apps/${AppId}`;
  origin: string;
  port: number;
};

/** Product order is persisted so a workspace addition cannot move existing ports. */
export const appDefinitions: readonly AppDefinition[] = [...productIds, ...sharedHostKeys].map((id, index) => ({
  id, package: `@erpx/app-${id}`, directory: `apps/${id}`,
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
