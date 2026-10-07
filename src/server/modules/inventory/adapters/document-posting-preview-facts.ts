import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { CountRow } from "../domain/counts";

export function documentPostingPreviewFacts(db: SqlDatabase) {
  return {
    stockCount: (companyId: string, id: string) => db.prepare("SELECT * FROM inventory_counts WHERE company_id=? AND id=?").bind(companyId, id).first<CountRow>(),
    stockDeltas: async (id: string) => (await db.prepare("SELECT counted_value_minor-snapshot_value_minor delta FROM inventory_count_lines WHERE count_id=?").bind(id).all<{ delta: number }>()).results,
  };
}
