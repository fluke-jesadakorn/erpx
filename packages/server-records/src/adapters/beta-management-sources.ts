import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { requestAiUsageProjection, requestAiUsageMonthlyCountSql } from './ai-usage-projection';
export function betaManagementRequestSources(db: SqlDatabase) {
    return {
        aiUsageSource: requestAiUsageProjection('company_id,month,input_tokens,output_tokens'),
        async requests(companyId: string, month: string) {
            return db.prepare(requestAiUsageMonthlyCountSql).bind(companyId, month).first<{
                count: number;
            }>();
        },
    };
}
