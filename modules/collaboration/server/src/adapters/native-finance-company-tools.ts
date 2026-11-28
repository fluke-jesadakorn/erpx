import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance company-tool statement. */
export function financeCompanyTools(db: SqlDatabase, companyId: string) {
  return {
    set: (f: { tool: string; enabled: boolean; version: number; userId: string; reason: string; now: number }): Promise<SqlStatement> =>
      nativeSystemNotificationStatement(db, companyId,
        { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind: 'finance_company_tool_set',
          values_json: JSON.stringify([companyId, f.tool, f.enabled ? 1 : 0, f.version, f.userId, f.reason, f.now]) }, '', 0),
  };
}
