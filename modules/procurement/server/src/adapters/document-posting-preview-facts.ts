import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { matchedBillLines, type BillRow } from "@erpx/procurement-server/domain/supplier-bills";
import { decimalToMinor } from "@erpx/finance-ui/lib/finance-money";
import { nativeProcurementReadFirst, PROCUREMENT_RD_DOCUMENT_POSTING_MATCHED_BILL } from "./native-procurement-read";

export function documentPostingPreviewFacts(db: SqlDatabase) {
  return {
    /** Owner-native read: planned by `RD_DOCUMENT_POSTING_MATCHED_BILL` and
     * executed on the caller's open unit; this module carries no SQL for it. */
    async matchedInvoiceLines(companyId: string, id: string) {
      const bill = await nativeProcurementReadFirst<BillRow>(
        PROCUREMENT_RD_DOCUMENT_POSTING_MATCHED_BILL, [id, companyId], db,
      );
      return bill ? matchedBillLines(bill).map(line => ({ id: line.accountId, debit: decimalToMinor(line.debit), credit: decimalToMinor(line.credit) })) : null;
    },
  };
}
