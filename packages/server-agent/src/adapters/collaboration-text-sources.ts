import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function collaborationTextSources(db: SqlDatabase) { return { async assistantName(companyId: string) { return db.prepare("SELECT 1").bind().first<{
        home_ai_name: string;
    }>(); }, async currentAssistantName(companyId: string) { return db.prepare("SELECT 1").bind().first<{
        home_ai_name: string;
    }>(); }, async previousAssistantName(companyId: string, label: string) { return db.prepare("SELECT 1").bind().first(); } }; }
