import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

export function activeLeavePolicySettings(db: SqlDatabase, companyId: string) {
  return db.prepare("SELECT name,version,status FROM leave_policy_versions WHERE company_id=? AND status='active' ORDER BY name,version DESC")
    .bind(companyId).all<{ name: string; version: number; status: string }>().then(result => result.results);
}
export function leavePolicySettingsHistory(db: SqlDatabase, companyId: string) {
  return db.prepare("SELECT name title,version,created_by actor,created_at at,'Leave policy' kind FROM leave_policy_versions WHERE company_id=? ORDER BY created_at DESC")
    .bind(companyId).all<{ title: string; version: number; actor: string; at: number; kind: string }>().then(result => result.results);
}
