import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeSupportReadAll, SUPPORT_RD_SERVICE_COVERAGE_HISTORY } from './native-support-read';

/** Owner-native read: the statement is planned by the Support native read
 * port (`RD_SERVICE_COVERAGE_HISTORY`) and executed on the caller's open unit.
 * This module carries no SQL for this read. */
export function serviceCoverageSettingsHistory(db: SqlDatabase, companyId: string) {
  return nativeSupportReadAll<{ title: string; version: number; actor: string; at: number; kind: string }>(
    SUPPORT_RD_SERVICE_COVERAGE_HISTORY,
    [companyId],
    db,
  ).then(result => result.results);
}
