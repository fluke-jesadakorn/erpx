import type { Clock } from "@erpx/server-shared/index";
import { holdingOperationStatus, operationTotals, unavailableHoldingOperations, type ControlServiceHealth, type HoldingJobStatistics, type HoldingOperationIdentity, type PlatformHoldingOperation, type PlatformOperationsSummary } from "@erpx/server-platform_administration/domain/operations-summary";

export interface PlatformOperationsRepository {
  controlHealth(): Promise<ControlServiceHealth[]>;
  jobs(holdingId: string, now: number, bindingIndex: number): Promise<HoldingJobStatistics>;
  migrations(holdingId: string, bindingIndex: number): Promise<PlatformHoldingOperation["migrations"]>;
}
export function createPlatformOperationsApplication(repository: PlatformOperationsRepository, clock: Clock) {
  return {
    async summarize(actorRole: string, holdings: HoldingOperationIdentity[]): Promise<PlatformOperationsSummary> {
      const now = clock.now(), controlHealth = await repository.controlHealth();
      const jobHeartbeatAt = controlHealth.find(health => health.service === "jobs")?.heartbeatAt ?? null;
      const rows = await Promise.all(holdings.map(async (holding: HoldingOperationIdentity, bindingIndex): Promise<PlatformHoldingOperation> => {
        try {
          const statistics = await repository.jobs(holding.id, now, bindingIndex);
          let migrations: PlatformHoldingOperation["migrations"];
          try { migrations = await repository.migrations(holding.id, bindingIndex); }
          catch { migrations = { track: "holding-write", applied: 0, pending: [], latestApplied: null, error: "Migration receipts are unavailable for this Holding." }; }
          return { ...holding, ...statistics, status: holdingOperationStatus(statistics), jobHeartbeatAt, error: null, migrations };
        } catch { return unavailableHoldingOperations(holding, jobHeartbeatAt); }
      }));
      return { generatedAt: now, actorRole, controlHealth, holdings: rows, totals: operationTotals(rows) };
    },
  };
}
