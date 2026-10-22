/** Fixed caller-bound projection of Management's original Company allowance. */
export const documentPageStorageLimitSql='COALESCE((SELECT storage_bytes FROM beta_policy WHERE company_id=?),9007199254740991)';
