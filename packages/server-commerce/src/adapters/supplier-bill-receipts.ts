import { commerceFinalOwnerProjectionSources as procurementProjectionSources } from "@erpx/server-procurement/public/commerce-final-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function stageSupplierBillReceiptVersion(db: SqlDatabase, id: string) { return db.prepare("SELECT 1").bind(); }
export function stageSupplierBillReceiptLinks(db: SqlDatabase, matchId: string, invoiceId: string | null) { return db.prepare("SELECT 1").bind(); }
