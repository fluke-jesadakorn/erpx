import 'server-only';
import {QUALITY_HOLD_SOURCE_SELECT} from './inventory-executive-owner-projections';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function inventoryQualitySnapshotSources(){return {
  "inventory_counts_inventory_quality_holds_from": `FROM (${QUALITY_HOLD_SOURCE_SELECT}) inventory_quality_holds`
} as const;}
