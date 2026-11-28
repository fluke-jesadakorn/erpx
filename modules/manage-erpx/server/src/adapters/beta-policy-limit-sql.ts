/** One byte-identical beta_policy Company-limit head shared by the storage-bytes
 * quota reader and the monthly AI allowance projection. Only a closed projection
 * token is admitted; the table and predicate stay here so neither reader can
 * drift and no request text is interpolated. */
export type BetaPolicyLimitProjection = 'storage_bytes' | 'ai_monthly_limit';
export const betaPolicyLimitSql = (projection: BetaPolicyLimitProjection) =>
  `SELECT ${projection} FROM beta_policy WHERE company_id=?`;
