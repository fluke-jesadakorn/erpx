import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

/** Native-owned company-business-acceptance notification statements. */
export function companyAcceptanceFacts(db: SqlDatabase, actor: WorkContext) {
  return {
    reviewerNotifications(
      key: string,
      _companyId: string,
      _companyId2: string,
      recordVersion: number,
      now: number,
      _companyId3: string,
      _userId: string,
      _now2: number,
      _now3: number,
    ): Promise<SqlStatement> {
      return nativeNotificationStatement(
        db,
        actor,
        {
          companyId: actor.companyId,
          userId: actor.userId,
          recordType: 'company_business_acceptance',
          recordId: actor.companyId,
          recordVersion,
          message: '',
          kind: 'company_acceptance_reviewer',
        },
        key,
        now,
      );
    },
    resultNotifications(
      key: string,
      value: 'Company setup acceptance was approved' | 'Company setup acceptance was returned for correction',
      now: number,
      _companyId: string,
    ): Promise<SqlStatement> {
      return nativeNotificationStatement(
        db,
        actor,
        {
          companyId: actor.companyId,
          userId: '',
          recordType: 'company_business_acceptance',
          recordId: actor.companyId,
          recordVersion: 1,
          message: value,
          kind: 'company_acceptance_result',
        },
        key,
        now,
      );
    },
  };
}
