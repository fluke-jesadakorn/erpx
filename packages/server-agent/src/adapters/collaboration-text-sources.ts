import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { agentHomeAiNameSql } from './collaboration-home-sources';
export function collaborationTextSources(db: SqlDatabase) { return { async assistantName(companyId: string) { return db.prepare(agentHomeAiNameSql).bind(companyId).first<{
        home_ai_name: string;
    }>(); }, async currentAssistantName(companyId: string) { return db.prepare(agentHomeAiNameSql).bind(companyId).first<{
        home_ai_name: string;
    }>(); }, async previousAssistantName(companyId: string, label: string) { return db.prepare("SELECT 1 FROM ai_home_name_history WHERE company_id=? AND name=?").bind(companyId, label).first(); } }; }
