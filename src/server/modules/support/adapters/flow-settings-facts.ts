import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

export function serviceCoverageSettingsHistory(db: SqlDatabase, companyId: string) {
  return db.prepare("SELECT t.name title,r.version,r.actor_id actor,r.created_at at,'Service coverage' kind FROM service_team_revisions r JOIN service_teams t ON t.id=r.team_id AND t.company_id=r.company_id WHERE r.company_id=? ORDER BY r.created_at DESC")
    .bind(companyId).all<{ title: string; version: number; actor: string; at: number; kind: string }>().then(result => result.results);
}
