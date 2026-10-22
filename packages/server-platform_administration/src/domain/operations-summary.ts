export type PlatformDeadLetter = { id: string; eventType: string; aggregateId: string; aggregateVersion: number; attempts: number; lastError: string | null; deadLetterAt: number };
export type PlatformHoldingOperation = {
  id: string; slug: string; displayName: string; status: "healthy" | "pending" | "attention" | "unavailable";
  pendingEvents: number; deadLetterEvents: number; claimedEvents: number; oldestPendingAt: number | null; jobHeartbeatAt: number | null; error: string | null;
  deadLetters: PlatformDeadLetter[];
  migrations: { track: "holding-write"; applied: number; pending: string[]; latestApplied: string | null; error: string | null };
};
export type ControlServiceHealth = { service: string; heartbeatAt: number; details: string };
export type PlatformOperationsSummary = { generatedAt: number; actorRole: string; controlHealth: ControlServiceHealth[]; holdings: PlatformHoldingOperation[]; totals: { holdings: number; pendingEvents: number; deadLetterEvents: number; claimedEvents: number } };
export type HoldingOperationIdentity = Pick<PlatformHoldingOperation, "id" | "slug" | "displayName">;
export type HoldingJobStatistics = Pick<PlatformHoldingOperation, "pendingEvents" | "deadLetterEvents" | "claimedEvents" | "oldestPendingAt" | "deadLetters">;

export function holdingOperationStatus(statistics: HoldingJobStatistics): PlatformHoldingOperation["status"] {
  return statistics.deadLetterEvents > 0 ? "attention" : statistics.pendingEvents > 0 || statistics.claimedEvents > 0 ? "pending" : "healthy";
}
export function unavailableHoldingOperations(holding: HoldingOperationIdentity, jobHeartbeatAt: number | null): PlatformHoldingOperation {
  const error = "Holding database or operational tables are unavailable.";
  return { ...holding, status: "unavailable", pendingEvents: 0, deadLetterEvents: 0, claimedEvents: 0, oldestPendingAt: null, jobHeartbeatAt, error, deadLetters: [], migrations: { track: "holding-write", applied: 0, pending: [], latestApplied: null, error } };
}
export function operationTotals(holdings: PlatformHoldingOperation[]): PlatformOperationsSummary["totals"] {
  return { holdings: holdings.length, pendingEvents: holdings.reduce((sum, row) => sum + row.pendingEvents, 0), deadLetterEvents: holdings.reduce((sum, row) => sum + row.deadLetterEvents, 0), claimedEvents: holdings.reduce((sum, row) => sum + row.claimedEvents, 0) };
}
