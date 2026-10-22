import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
/** Include this source version mutation in the supplied Procurement command transaction. */
export function stageSupplierPromiseOrderVersion(db: SqlDatabase, id: string, now: number) { return db.prepare("UPDATE commerce_orders SET version=version+1,updated_at=? WHERE id=?").bind(now, id); }
