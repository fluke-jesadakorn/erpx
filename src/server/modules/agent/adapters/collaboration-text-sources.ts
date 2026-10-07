import type { SqlDatabase } from '../../runtime/public/runtime/types';
export function collaborationTextSources(db: SqlDatabase) { return { async assistantName(companyId: string) { return db.prepare("SELECT home_ai_name FROM ai_company_settings WHERE company_id=?").bind(companyId).first<{
        home_ai_name: string;
    }>(); }, async currentAssistantName(companyId: string) { return db.prepare("SELECT home_ai_name FROM ai_company_settings WHERE company_id=?").bind(companyId).first<{
        home_ai_name: string;
    }>(); }, async previousAssistantName(companyId: string, label: string) { return db.prepare("SELECT 1 FROM ai_home_name_history WHERE company_id=? AND name=?").bind(companyId, label).first(); } }; }
