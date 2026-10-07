import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

export function departmentFlowSettingsHistory(db: SqlDatabase, companyId: string, departmentIds: string[]) {
  return db.prepare("SELECT department_id,version,published_by,reason,created_at FROM department_flow_versions WHERE company_id=? AND department_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY created_at DESC,department_id,version DESC")
    .bind(companyId, JSON.stringify(departmentIds)).all<{
      department_id: string; version: number; published_by: string; reason: string; created_at: number;
    }>().then(result => result.results);
}
