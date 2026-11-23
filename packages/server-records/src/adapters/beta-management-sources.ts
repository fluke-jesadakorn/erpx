import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { requestAiUsageMonthlyCountSql } from './ai-usage-projection';
export function betaManagementRequestSources(db: SqlDatabase) {
    return {
        aiUsageSource: 'SELECT company_id,month,input_tokens,output_tokens FROM request_ai_usage',
        async requests(companyId: string, month: string) {
            return db.prepare(requestAiUsageMonthlyCountSql).bind(companyId, month).first<{
                count: number;
            }>();
        },
    };
}
