/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { currentReadMetrics, runWithReadMetrics, recordReadQuery, recordReadPoolWait, measureReads, measureReadPhase } from "../../../../runtime/read-metrics";
export type { ReadMetrics, ReadMeasurement } from "../../../../runtime/read-metrics";
