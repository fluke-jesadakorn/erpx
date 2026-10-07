import type { SqlDatabase } from '../../runtime/public/runtime/types';
export function collaborationHomeSources(db: SqlDatabase) {
    return { async aiName(companyId: string) {
            return db.prepare("SELECT home_ai_name FROM ai_company_settings WHERE company_id=?").bind(companyId).first<{
                home_ai_name: string;
            }>();
        } };
}
