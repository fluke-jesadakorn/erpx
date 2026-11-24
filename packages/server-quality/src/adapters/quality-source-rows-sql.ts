import "server-only";
/** Owner-local full-row source read shared by the Quality settlement package
 * sources. The generated SQL is byte-identical to the former inline full-row
 * reads for the same table argument. No bind value or caller SQL is
 * interpolated: table is a fixed owner-owned table name. */
export const qualitySourceRows = (table: string) => `SELECT * FROM ${table}`;
