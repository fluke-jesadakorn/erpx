import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance reporting and year-closing statements. */
export function nativeFinanceReporting(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    statementPackage: (v: unknown[]) => call('finance_statement_package_insert', v),
    statementEventPrepared: (v: unknown[]) => call('finance_statement_event_prepared', v),
    statementPackageDecision: (v: unknown[]) => call('finance_statement_package_decision', v),
    statementEventDecision: (v: unknown[]) => call('finance_statement_event_decision', v),
    yearClosingInsert: (v: unknown[]) => call('finance_year_closing_insert', v),
    yearClosingDecision: (v: unknown[]) => call('finance_year_closing_decision', v),
  };
}
