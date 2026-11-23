import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { SqlValueRow } from '@erpx/server-runtime/public/runtime/sql';
import { nativePosAll, nativePosOne } from './native-owner-write';

/**
 * Owner-issued POS core workspace reads (shifts family) for caller-owned
 * transactions. The SQL lives in the native POS owner registry
 * (`erpx-server-pos::adapters::core_reads`); this bridge sends a typed kind and
 * positional values only. The caller executes the statement on its own unit,
 * so no TypeScript SQL, mirror or fallback reader exists for this family.
 */

type Scope = { company: string; branch: string | null; branch2: string | null };

/** Owner-issued shift list with the cash-expected projection. */
export async function nativePosShiftsStatement(db: SqlDatabase, scope: Scope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, 'pos_core_shifts', [scope.company, scope.branch, scope.branch2]);
}

/** Owner-issued restaurant table list. */
export async function nativePosTablesStatement(db: SqlDatabase, scope: Scope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, 'pos_core_tables', [scope.company, scope.branch, scope.branch2]);
}

/** Owner-issued cash-movement list joined to its shift register. */
export async function nativePosCashMovementsStatement(db: SqlDatabase, scope: Scope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, 'pos_core_cash_movements', [scope.company, scope.branch, scope.branch2]);
}

export type PosSalesEvidenceScope = {
  id: string;
  company: string;
  branch: string | null;
  branch2: string | null;
  currency: string;
  from: string;
  to: string;
};

/**
 * Owner-issued executive-evidence sale row. The SQL lives in the native POS
 * owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpossales3_520`); this bridge sends only the typed kind and scoped
 * values. It replaces the two byte-identical `readPosSales3`/`readPosSales4`
 * TypeScript literals in `pos.ts`, so no local SQL, mirror or fallback remains.
 */
export async function nativePosSalesEvidenceStatement(db: SqlDatabase, facts: PosSalesEvidenceScope): Promise<SqlValueRow | null> {
  return nativePosOne(db, facts.company, 'core_readpossales3_520', [
    facts.id, facts.company, facts.branch, facts.branch2, facts.currency, facts.from, facts.to,
  ]);
}

/**
 * Owner-issued plain by-id sale row. The SQL lives in the native POS owner
 * registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpossalesbyid_521`); this bridge sends only the typed kind and the
 * sale id. It replaces the three byte-identical by-id `pos_sales` row literals
 * in `pos-core.ts`, `pos-daily.ts` and `pos-stripe.ts`, so no
 * local SQL, mirror or fallback remains. The read is scoped by sale id alone;
 * no actor/Company threading is required.
 */
export async function nativePosSaleByIdStatement(db: SqlDatabase, saleId: unknown): Promise<SqlValueRow | null> {
  return nativePosOne(db, '', 'core_readpossalesbyid_521', [saleId]);
}

/**
 * Owner-issued settings-currency read. The SQL lives in the native POS owner
 * registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpossettingscurrency_522`); this bridge sends only the typed kind and
 * the Company value. It replaces the two byte-identical settings-currency
 * row literals in `pos-daily.ts`, so no local SQL, mirror or fallback reader
 * remains.
 */
export async function nativePosSettingsCurrencyStatement(db: SqlDatabase, company: string): Promise<SqlValueRow | null> {
  return nativePosOne(db, company, 'core_readpossettingscurrency_522', [company]);
}

/**
 * Owner-issued posting-journal id read. The SQL lives in the native POS owner
 * registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpospostingsjournal_523`); this bridge sends only the typed kind and
 * the posting source key. It replaces the two byte-identical journal-id row
 * literals in `pos-core.ts` and `pos-stripe.ts`, so no local SQL, mirror or
 * fallback remains. The read is scoped by source key alone; no actor/Company
 * threading is required.
 */
export async function nativePosPostingJournalStatement(db: SqlDatabase, sourceKey: unknown): Promise<SqlValueRow | null> {
  return nativePosOne(db, '', 'core_readpospostingsjournal_523', [sourceKey]);
}

/**
 * Owner-issued register-catalog membership read. The SQL lives in the native
 * POS owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readposregistercatalog_524`); this bridge sends only the typed kind and
 * the product, branch and Company values. It replaces the two byte-identical
 * `readPosRegisterCatalog1` SQL literals in `pos-core.ts` and `pos-images.ts`,
 * so no local SQL, mirror or fallback remains. Caller-transaction read scoped
 * by product, branch and Company.
 */
export async function nativePosRegisterCatalogStatement(
  db: SqlDatabase,
  productId: string,
  branchId: string,
  company: string,
): Promise<SqlValueRow | null> {
  return nativePosOne(db, company, 'core_readposregistercatalog_524', [productId, branchId, company]);
}
