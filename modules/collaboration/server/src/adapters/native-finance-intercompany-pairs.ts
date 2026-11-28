import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance intercompany pair statements. */
export function financeIntercompanyPairs(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    createPair: (f: {
      id: string; holdingId: string; companyId: string; reference: string;
      sourceCompanyId: string; sourceJournalId: string; sourceAccountId: string;
      targetCompanyId: string; targetJournalId: string; targetAccountId: string;
      currency: string; amountMinor: string; sourceJson: string; sourceHash: string;
      userId: string; reason: string; now: number;
    }) => call('finance_intercompany_pair_create', f.id,
      [f.id, f.holdingId, f.companyId, f.reference, f.sourceCompanyId, f.sourceJournalId, f.sourceAccountId,
       f.targetCompanyId, f.targetJournalId, f.targetAccountId, f.currency, f.amountMinor, f.sourceJson, f.sourceHash, f.userId, f.reason, f.now, f.now]),
    claim: (f: { pairId: string; journalId: string; accountId: string }) =>
      call('finance_intercompany_claim', '', [f.pairId, f.journalId, f.accountId]),
    decide: (f: { status: string; userId: string; now: number; id: string }) =>
      call('finance_intercompany_pair_decide', '', [f.status, f.userId, f.now, f.id]),
    clearClaims: (f: { pairId: string }) => call('finance_intercompany_claims_clear', '', [f.pairId]),
  };
}
