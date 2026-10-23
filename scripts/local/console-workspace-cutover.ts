import { appDefinitions } from "@erpx/contracts/apps";

/** Read-only development readiness, never production acceptance or authority. */
export function consoleWorkspaceCutoverBlockers(activeApps: readonly string[], environment: Readonly<Record<string, string | undefined>>): string[] {
  const blockers: string[] = [];
  if (environment.ERPX_ENVIRONMENT !== "local" || environment.ERPX_PRODUCT_ENVIRONMENT !== "development") blockers.push("Development environment required");
  if (!/^https:\/\/console-dev-erpx\.jesadakorn\.com\/?$/.test(environment.ERPX_PUBLIC_ORIGIN ?? "")) blockers.push("Exact development Console origin required");
  for (const key of ["ERPX_PRODUCT_ROUTING", "ERPX_PRODUCT_SSO"])
    if (environment[key] !== "enabled") blockers.push(`${key} is not enabled`);
  if (new Set(activeApps).size !== activeApps.length) blockers.push("Duplicate active application mappings");
  for (const id of activeApps) if (!appDefinitions.some(app => app.id === id)) blockers.push(`Unknown application mapping: ${id}`);
  for (const app of appDefinitions) if (!activeApps.includes(app.id)) blockers.push(`Application still uses compatibility mapping: ${app.id}`);
  return blockers;
}
