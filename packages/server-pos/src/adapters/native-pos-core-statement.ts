import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { SqlValueRow } from '@erpx/server-runtime/public/runtime/sql';
import { nativePosAll } from './native-owner-write';

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
