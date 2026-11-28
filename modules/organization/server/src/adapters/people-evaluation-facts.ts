import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  nativeLane24LevelActive,
  nativeLane24MarketLevels,
  nativeLane24PositionByLevel,
  nativeLane24WorkspaceLevels,
  nativeLane24WorkspacePositions,
  type Lane24Actor,
} from './native-lane-24/client';

/**
 * Organization-owned evaluation reference facts. The SQL, Company-scoped bind,
 * projection and response shape are owned by the native lane-24 axum router.
 * The caller supplies a WorkContext-derived target (or an explicit registered
 * host/origin); with none, the client fails closed. This adapter never reads
 * ambient request state and prepares no SQL.
 */
const asResult = <T>(rows: T[]) => ({
  results: rows,
  success: true,
  meta: { changes: 0, duration: 0 },
});

export function peopleEvaluationOrganizationFacts(_database: SqlDatabase, target?: Lane24Actor) {
  return {
    async readSaveEvaluationDefinitionPosition(positionId: string, companyId: string, levelId: string) {
      return nativeLane24PositionByLevel(target, companyId, positionId, levelId);
    },
    async readSaveEvaluationDefinitionEffect(levelId: string, companyId: string) {
      return nativeLane24LevelActive(target, companyId, levelId);
    },
    async readEvaluationWorkspacePositions(companyId: string) {
      return nativeLane24WorkspacePositions(target, companyId).then(asResult);
    },
    async readEvaluationWorkspaceLevels(companyId: string) {
      return nativeLane24WorkspaceLevels(target, companyId).then(asResult);
    },
    async readImportEvaluationMarketLevels(companyId: string) {
      return nativeLane24MarketLevels(target, companyId).then(asResult);
    },
  };
}
