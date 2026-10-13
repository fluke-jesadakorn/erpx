/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { currentReadMetrics, runWithReadMetrics, recordReadQuery, recordReadPoolWait, recordReadLockQuery, recordReadTransactionRetry, measureReads, measureReadPhase } from "@/server/modules/runtime/adapters/runtime/read-metrics";
export type { ReadMetrics, ReadMeasurement } from "@/server/modules/runtime/adapters/runtime/read-metrics";
