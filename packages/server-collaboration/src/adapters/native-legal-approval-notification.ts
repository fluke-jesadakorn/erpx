import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

export type LegalReviewerNotice = {
  identifier: string;
  companyId: string;
  approver_user_id: string;
  itemId: string;
  nextVersion: number;
  now: number;
};

/** Native-owned Legal approval reviewer notice statement. */
export function nativeLegalApprovalNotification(
  db: SqlDatabase,
  actor: WorkContext,
  facts: LegalReviewerNotice,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: facts.companyId,
      userId: facts.approver_user_id,
      recordType: 'legal',
      recordId: facts.itemId,
      recordVersion: facts.nextVersion,
      message: 'Legal approval needs your decision',
      kind: 'legal_approval',
      identifier: facts.identifier,
      approver_user_id: facts.approver_user_id,
      item_id: facts.itemId,
    },
    facts.identifier,
    facts.now,
  );
}
