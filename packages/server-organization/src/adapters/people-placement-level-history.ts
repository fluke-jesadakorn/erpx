import type { SqlResult } from "@erpx/server-runtime/public/runtime/types";
import { nativeOrgExecute } from "./native-organization-owner-write";

/** Record a placement level history fact through the native owner executor.
 * The result shape matches the retained `SqlStatement.run()` contract. */
export async function recordPeoplePlacementLevelHistory(
  _db: unknown,
  id: string,
  engagementId: string,
  previousLevelId: string | null,
  levelId: string | null,
  actorId: string,
  reason: string,
  commandId: string,
  now: number,
  companyId = '',
): Promise<SqlResult> {
  const result = await nativeOrgExecute(
    companyId,
    'staff_level_history_insert',
    [id, engagementId, previousLevelId, levelId, actorId, reason, commandId, now],
    commandId,
    now,
  );
  return { results: [], success: true, meta: { changes: result.rows, duration: 0 } };
}
