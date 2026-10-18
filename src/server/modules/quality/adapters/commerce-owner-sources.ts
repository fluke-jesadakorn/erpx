import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Basic protective receipt/return quarantine is an original Commerce effect,
 * including without advanced Quality purchase. Specifications stay read-only. */
export function commerceOwnerQualitySourceReadLocks(database:SqlDatabase){return [
 database.prepare("LOCK TABLE inventory_quality_holds IN SHARE ROW EXCLUSIVE MODE"),
 database.prepare("LOCK TABLE inventory_quality_specs,inventory_quality_spec_metrics,inventory_quality_disposition_events IN SHARE MODE")
];}
