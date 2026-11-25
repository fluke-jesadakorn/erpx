/** Byte-identical PMS executive-metric label head shared by the occupancy,
 * ADR/RevPAR and guest-account-balance projections. Text only: every caller
 * keeps its own label expression, tail, binds and result type; this module
 * only recomposes the fixed head so the three sites cannot drift. The
 * blockers projection keeps its distinct inline `label,count(*)` head. */
export const pmsExecutiveMetricLabelHeadSql = (label: string): string => `SELECT ${label} label,`;
