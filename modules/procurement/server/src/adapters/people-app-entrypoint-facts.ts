import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeProcurementReadFirst, PROCUREMENT_RD_REQUEST_QUOTATION } from './native-procurement-read';

/** Owner-native quotation lookup. The statement is planned by the Procurement
 * native read port (`RD_REQUEST_QUOTATION`) and executed on the caller's open
 * unit; this module carries no SQL for this read. */
export function requestQuotation(db: SqlDatabase, requestId: string, companyId: string) {
  return nativeProcurementReadFirst<{ id: string }>(
    PROCUREMENT_RD_REQUEST_QUOTATION, [requestId, companyId], db,
  );
}
