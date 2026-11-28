import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
import {type Row} from '@erpx/commerce-server/public/commerce';
import {
  nativeInventoryReadAll,
  nativeInventoryReadFirst,
  INVENTORY_RD_LEDGER_SUPPLEMENT_ROWS,
  INVENTORY_RD_LOT_BALANCE_SUPPLEMENT_ROWS,
  INVENTORY_RD_LEDGER_SUPPLEMENT_ROW,
  INVENTORY_RD_LOT_SUPPLEMENT_ROW,
  INVENTORY_RD_STOCK_SUPPLEMENT_ROW,
} from './native-inventory-read';
export function RecordDetailsInventoryOwnedFacts(database: SqlDatabase) {
    return {
        /** Owner-native read: planned by `RD_LEDGER_SUPPLEMENT_ROW` and executed
         * on the caller's unit; this module carries no SQL for it. */
        readRecordSupplementDataRow(id: string, companyId: string, branchId: string | null, branchId2: string | null) { return nativeInventoryReadFirst<Row>(INVENTORY_RD_LEDGER_SUPPLEMENT_ROW, [id, companyId, branchId, branchId2], database); },
        /** Owner-native read: planned by `RD_LOT_SUPPLEMENT_ROW` and executed on
         * the caller's unit; this module carries no SQL for it. */
        readRecordSupplementDataRow2(id: string, companyId: string, companyId2: string, branchId: string | null, branchId2: string | null) { return nativeInventoryReadFirst<Row>(INVENTORY_RD_LOT_SUPPLEMENT_ROW, [id, companyId, companyId2, branchId, branchId2], database); },
        /** Owner-native read: planned by `RD_LOT_BALANCE_SUPPLEMENT_ROWS` and
         * executed on the caller's unit; this module carries no SQL for it. */
        readRecordSupplementDataRows(id: string, companyId: string, branchId: string | null, branchId2: string | null, offset: number) { return nativeInventoryReadAll<Row>(INVENTORY_RD_LOT_BALANCE_SUPPLEMENT_ROWS, [id, companyId, branchId, branchId2, offset], database); },
        /** Owner-native read: planned by `RD_STOCK_SUPPLEMENT_ROW` and executed
         * on the caller's unit; this module carries no SQL for it. */
        readRecordSupplementDataRow3(id: string, warehouseId: string, companyId: string, branchId: string | null, branchId2: string | null) { return nativeInventoryReadFirst<Row>(INVENTORY_RD_STOCK_SUPPLEMENT_ROW, [id, warehouseId, companyId, branchId, branchId2], database); },
        /** Owner-native read: the SQL is planned by the Inventory native read
         * port (`RD_LEDGER_SUPPLEMENT_ROWS`) and executed on the caller's unit;
         * this module carries no SQL for this read. */
        readRecordSupplementDataRows2(warehouseId: string, id: string, offset: number) { return nativeInventoryReadAll<Row>(INVENTORY_RD_LEDGER_SUPPLEMENT_ROWS, [warehouseId, id, offset], database); } };
}
