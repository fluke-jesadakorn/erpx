import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance book statements. */
export function nativeFinanceBooks(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    openingJournal: (v: unknown[]) => call('books_opening_journal', v),
    openingJournalLines: (v: unknown[]) => call('books_opening_journal_lines', v),
    ruleDecision: (v: unknown[]) => call('books_rule_decision', v),
    dimensionDefinition: (v: unknown[]) => call('books_dimension_definition', v),
    dimensionValue: (v: unknown[]) => call('books_dimension_value', v),
    rulePrepare: (v: unknown[]) => call('books_rule_prepare', v),
    accountMapping: (v: unknown[]) => call('books_account_mapping', v),
  };
}
