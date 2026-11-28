import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import contract from "./finance-lineage-sources.json" with { type: "json" };

/** Native Field finance lineage/history port for the Finance purchase
 * adapters. The exact SQL is owned by the Rust `erpx-server-field-service`
 * owner in `finance-lineage-sources.json` (bound into Rust with `include_str!`
 * and exposed by `public::finance_lineage::field_finance_source`); this module
 * carries no SQL literal and accepts no caller-supplied relation or statement.
 *
 * The fixed source identities are the immutable billing release, event,
 * command receipt, money snapshot and consumption rows. Caller source labels
 * grant nothing. Removal condition for this port: the remaining Finance
 * purchase adapters move to the mounted native journal/invoice/workspace
 * authority, after which this contract and its PG parity oracles retire. */
export const nativeFieldFinanceLineage = contract as {
  readonly invoice: string;
  readonly partsJournal: string;
  readonly payment: string;
  readonly journal: string;
  readonly sourceReadLocks: readonly string[];
};

/** Backwards-compatible name for the still-unswitched Finance PG oracle.
 * New TypeScript consumers use `nativeFieldFinanceLineage` above. */
export const fieldServiceFinanceRecordSources = {
  invoice: nativeFieldFinanceLineage.invoice,
  partsJournal: nativeFieldFinanceLineage.partsJournal,
  payment: nativeFieldFinanceLineage.payment,
  journal: nativeFieldFinanceLineage.journal,
} as const;

/** Fixed owner leases before the first Finance/Files source snapshot. Service
 * starts with the same Support relation, so an owner writer never holds later
 * source leases while waiting to upgrade its earlier case relation. */
export function nativeFieldFinanceSourceReadLocks(database:SqlDatabase){return nativeFieldFinanceLineage.sourceReadLocks.map(sql=>database.prepare(sql));}
/** Retained alias: same native fixed leases under the historical export name. */
export const fieldServiceFinanceSourceReadLocks = nativeFieldFinanceSourceReadLocks;
