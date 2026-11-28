import 'server-only';
import type { SqlDatabase, SqlResult } from '@erpx/platform-runtime-server/public/runtime/types';
import type { SqlValueRow } from '@erpx/platform-runtime-server/public/runtime/sql';
import { nativePosAll, nativePosOne, nativePosStatement } from './native-owner-write';

/**
 * Owner-issued POS list reads (round 31) on caller-owned transactions.
 * The SQL for every kind lives in the native POS owner registry
 * (`erpx-server-pos::adapters::list_reads`); this bridge sends only a typed kind
 * and the positional values. The caller executes the statement on its own unit,
 * so no TypeScript SQL, mirror or fallback reader remains for this family.
 * Row-locked reads, PMS fragments and the day-report/Stripe registries stay
 * inline in their callers and modules.
 */

const SALES = 'pos_list_sales';
const REFUNDS = 'pos_list_refunds';
const CONNECTIONS = 'pos_list_connections';
const SETTLEMENTS = 'pos_list_settlements';
const EXCEPTION_JOBS = 'pos_list_exception_jobs';
const RECORD_SUPPLEMENT_SALES = 'pos_list_record_supplement_sales';

/** Workspace sales list, bounded by Company, Branch, number and state. */
export async function nativePosSalesListStatement(
  db: SqlDatabase,
  facts: {
    company: string;
    branch: string | null;
    branch2: string | null;
    fact3: string;
    fact4: string;
    fact5: 0 | 1;
  },
): Promise<SqlValueRow[]> {
  return nativePosAll(db, facts.company, SALES, [
    facts.company, facts.branch, facts.branch2, facts.fact3, facts.fact4, facts.fact5,
  ]);
}

/** Workspace refunds list, bounded by Company, Branch and sale id. */
export async function nativePosRefundsListStatement(
  db: SqlDatabase,
  facts: {
    company: string;
    branch: string | null;
    branch2: string | null;
    fact3: string | number | boolean | null | undefined;
    fact4: string | number | boolean | null | undefined;
  },
): Promise<SqlValueRow[]> {
  return nativePosAll(db, facts.company, REFUNDS, [
    facts.company, facts.branch, facts.branch2, facts.fact3, facts.fact4,
  ]);
}

/** PMS booking connection-readiness row for a Company. */
export async function nativePosConnectionsRowStatement<T = SqlValueRow>(
  db: SqlDatabase,
  company: string,
): Promise<T | null> {
  return nativePosOne<T>(db, company, CONNECTIONS, [company]);
}

/** Settlements list for a Company. */
export async function nativePosSettlementsListStatement(
  db: SqlDatabase,
  company: string,
): Promise<SqlValueRow[]> {
  return nativePosAll(db, company, SETTLEMENTS, [company]);
}

/** Exception/refund-connect jobs list for a Company, with refund revision. */
export async function nativePosExceptionJobsListStatement(
  db: SqlDatabase,
  company: string,
): Promise<SqlValueRow[]> {
  return nativePosAll(db, company, EXCEPTION_JOBS, [company]);
}

/**
 * POS-owned sale-id list for the Records owner's record-supplement read. The
 * SQL lives in the native POS owner registry (`erpx-server-pos::adapters::
 * list_reads`, kind `pos_list_record_supplement_sales`) and is byte-identical to
 * the retired literal. The caller executes it on its own unit and retains the
 * original `.results` contract; no local SQL, mirror or fallback remains.
 */
export async function nativePosRecordSupplementSalesStatement<T = SqlValueRow>(
  db: SqlDatabase,
  facts: {
    companyId: string;
    branchId: string | null;
    branchId2: string | null;
    sourceId: string | number | boolean | null;
    sourceId2: string | number | boolean | null;
    sourceId3: string | number | boolean | null;
  },
): Promise<SqlResult<T>> {
  return (await nativePosStatement(db, facts.companyId, RECORD_SUPPLEMENT_SALES, [
    facts.companyId, facts.branchId, facts.branchId2,
    facts.sourceId, facts.sourceId2, facts.sourceId3,
  ])).all<T>();
}
