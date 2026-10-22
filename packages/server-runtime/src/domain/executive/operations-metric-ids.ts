export const operationsMetrics = [
  "open-operations-exceptions",
  "exception-resolution-time",
  "operations-escalations",
] as const;

export type OperationsMetricId = (typeof operationsMetrics)[number];