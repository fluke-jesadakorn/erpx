/** Native projection wrapper shared by the Quality/Ecommerce settlement source reads. */
export const nativeSourceWhere = (source: string, predicate: string) => `SELECT native.* FROM (${source}) native WHERE ${predicate}`;
