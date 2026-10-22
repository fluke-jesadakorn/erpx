import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function prepareCustomerAuthorityCommerceMerge(write: SqlDatabase, canonical: string, loser: string, now: number) { return write.prepare('UPDATE commerce_orders SET party_id=?,version=version+1,updated_at=? WHERE party_id=?').bind(canonical, now, loser); }
