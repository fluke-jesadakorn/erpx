/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { currentReadMetrics, runWithReadMetrics, recordReadQuery, recordReadPoolWait, recordReadLockQuery, recordReadTransactionRetry, measureReads, measureReadPhase } from "@erpx/platform-runtime-server/adapters/runtime/read-metrics";
export type { ReadMetrics, ReadMeasurement } from "@erpx/platform-runtime-server/adapters/runtime/read-metrics";
