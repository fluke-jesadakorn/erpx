import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function collaborationHomeSources(db: SqlDatabase) {
    return { async aiName(companyId: string) {
            return db.prepare("SELECT 1").bind().first<{
                home_ai_name: string;
            }>();
        } };
}
