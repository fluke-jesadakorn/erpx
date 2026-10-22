import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function betaManagementRequestSources(db: SqlDatabase) {
    return {
        aiUsageSource: 'SELECT company_id,month,input_tokens,output_tokens FROM request_ai_usage',
        async requests(companyId: string, month: string) {
            return db.prepare('SELECT count(*)::int count FROM request_ai_usage WHERE company_id=? AND month=?').bind(companyId, month).first<{
                count: number;
            }>();
        },
    };
}
