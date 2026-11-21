import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { nativeOwnerStatement } from "@erpx/server-commerce/adapters/native-owner-write";

/** Include this source version mutation in the supplied Procurement command
 * batch. The returned object carries no SQL: its run() executes the version
 * bump through the native Commerce axum command route in Rust. */
export function stageSupplierPromiseOrderVersion(db: SqlDatabase, id: string, now: number): SqlStatement {
  return nativeOwnerStatement('supplier.order-version', [now, id], id, 'system', `supplier:order-version:${id}:${now}`, id);
}
