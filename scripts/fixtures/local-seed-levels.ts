import type { SqlDatabase } from "../../src/server/runtime/types";
import { staffLevelTitles } from "../../src/server/modules/organization/public/rules/organization-domain";

/** Install only missing numbered prerequisites; existing custom IDs and values remain authoritative. */
export async function localSeedLevels(db: SqlDatabase, companyId: string, now: number): Promise<Map<number, string>> {
  await db.batch(staffLevelTitles.map((name, index) => db.prepare(`INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,created_at,updated_at)
    SELECT ?,?,?,?,?,?,1,?,? WHERE NOT EXISTS(SELECT 1 FROM staff_levels WHERE company_id=? AND level_number=?) ON CONFLICT DO NOTHING`)
    .bind(`org-level-${companyId}-${index + 1}`, companyId, `ORG_L${index + 1}`, name, -index - 1, index + 1, now, now, companyId, index + 1)));
  const rows = (await db.prepare("SELECT id,level_number,active FROM staff_levels WHERE company_id=? AND level_number BETWEEN 1 AND 6 ORDER BY level_number")
    .bind(companyId).all<{ id: string; level_number: number; active: number }>()).results;
  if (rows.length !== 6 || rows.some(row => row.active !== 1)) throw new Error(`LOCAL_SEED_NUMBERED_LEVELS_REQUIRED:${companyId}`);
  return new Map(rows.map(row => [row.level_number, row.id]));
}
