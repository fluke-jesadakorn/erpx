import "server-only";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { lockPackageSettlementSources } from "@erpx/server-commercial/adapters/package-locks";

declare const sourceLock: unique symbol;
/** Minted only while a fresh owned transaction is acquiring all source locks.
 * Consumers cannot attach an owner reader to a previously established snapshot. */
export type PackageSettlementSourceLock = { readonly [sourceLock]: true };
const locks = new WeakMap<PackageSettlementSourceLock, { owner: PostgresDatabase; database: SqlDatabase; companyId: string }>();
export function packageSettlementSource(owner: PostgresDatabase, lock: PackageSettlementSourceLock) {
  const source = locks.get(lock);
  if (!source || source.owner !== owner) throw new Error("PACKAGE_SETTLEMENT_SOURCE_LOCK_REQUIRED");
  owner.assertActiveTransaction(source.database);
  return { database: source.database, companyId: source.companyId };
}
export function withPackageSettlementSources<Result>(owner: PostgresDatabase, companyId: string,
  sources: readonly ((lock: PackageSettlementSourceLock) => Promise<void>)[], run: (transaction: SqlDatabase) => Promise<Result>) {
  if (!companyId || !sources.length) throw new Error("PACKAGE_SETTLEMENT_SOURCES_REQUIRED");
  return owner.transaction(async transaction => {
    await lockPackageSettlementSources(transaction);
    const lock = {} as PackageSettlementSourceLock;
    locks.set(lock, { owner, database: transaction, companyId });
    try { for (const source of sources) await source(lock); }
    finally { locks.delete(lock); }
    return run(transaction);
  });
}
