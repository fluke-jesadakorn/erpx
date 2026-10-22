import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

export function spendingSettingsHistory(db: SqlDatabase, companyId: string) {
  return db.prepare("SELECT 'Company spending rules' title,record_version version,actor_id actor,created_at at,'Spending rules revision' kind FROM audit_events WHERE action='spending.template.save' AND record_id=? AND authority_json::jsonb->>'companyId'=? ORDER BY created_at DESC")
    .bind(companyId, companyId).all<{ title: string; version: number; actor: string; at: number; kind: string }>().then(result => result.results);
}
