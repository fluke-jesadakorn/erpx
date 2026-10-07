import { BusinessRuleError } from "../../shared/domain/error";
export type QualityMetric = { id: string | number | null; minimum_value: string | number | null; maximum_value: string | number | null };
export type QualityMeasurement = { metricId: string; value: string };
export function qualityMeasurementPassed(metric: QualityMetric, value: string): boolean {
  const measured = Number(value);
  return Number.isFinite(measured) && (metric.minimum_value === null || measured >= Number(metric.minimum_value)) && (metric.maximum_value === null || measured <= Number(metric.maximum_value));
}
export function evaluateQualityMeasurements(metrics: QualityMetric[], measurements: QualityMeasurement[], requested?: "pass" | "fail") {
  const provided = new Map(measurements.map((measurement) => [measurement.metricId, measurement.value]));
  let result = requested;
  if (metrics.length) {
    if (provided.size !== measurements.length || provided.size !== metrics.length || metrics.some((metric) => !provided.has(String(metric.id)))) throw new BusinessRuleError(422, "QUALITY_MEASUREMENTS_REQUIRED", "Record one value for every measurement in the held specification.");
    const derived = metrics.every((metric) => qualityMeasurementPassed(metric, provided.get(String(metric.id))!)) ? "pass" : "fail";
    if (result && result !== derived) throw new BusinessRuleError(422, "QUALITY_RESULT_DERIVED", "The inspection result is calculated from the recorded limits.");
    result = derived;
  } else {
    if (measurements.length) throw new BusinessRuleError(422, "QUALITY_SPEC_REQUIRED", "This hold has no measurement specification.");
    if (!result) throw new BusinessRuleError(422, "QUALITY_RESULT_REQUIRED", "Choose pass or fail for a hold without a structured specification.");
  }
  return { provided, result };
}
