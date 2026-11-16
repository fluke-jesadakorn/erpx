import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
/** Include this source version mutation in the supplied Procurement command transaction. */
export function stageSupplierPromiseOrderVersion(db: SqlDatabase, id: string, now: number) { return db.prepare("SELECT 1").bind(); }
