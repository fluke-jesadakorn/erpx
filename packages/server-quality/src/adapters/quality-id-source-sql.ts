/** Owner-local id-source membership projection shared by the Quality/Ecommerce customer-return settlement reads. */
export const qualityIdInSourceSql = (source: string, alias: string) => `IN (SELECT id FROM (${source}) ${alias})`;
