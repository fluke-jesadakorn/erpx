import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/** Acquire before Company advisory locks or owner row/table locks. Ordinary
 * writers coexist; a settlement snapshot excludes them before reading sources.
 * LOCK establishes no SERIALIZABLE data snapshot, including when it must wait. */
export function packageWriterFence(database: SqlDatabase) {
  return database.prepare("LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE");
}
export async function lockPackageSettlementSources(database: SqlDatabase) {
  await database.prepare("LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE").run();
}
