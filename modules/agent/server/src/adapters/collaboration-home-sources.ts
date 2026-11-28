import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
/** The one owner-authored fixed SQL for the Agent home AI name read. Every
 * home-source caller reuses this text; no adapter authors its own literal. */
export const agentHomeAiNameSql = "SELECT home_ai_name FROM ai_company_settings WHERE company_id=?";
export function collaborationHomeSources(db: SqlDatabase) {
    return { async aiName(companyId: string) {
            return db.prepare(agentHomeAiNameSql).bind(companyId).first<{
                home_ai_name: string;
            }>();
        } };
}
