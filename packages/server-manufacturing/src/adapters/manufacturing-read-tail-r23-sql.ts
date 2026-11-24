/** Byte-identical Manufacturing read-tail r23 SQL text shared by the two
 * stock-footprint projection branches of the package-settlement sources.
 * Text only: every caller keeps its own projected columns, joined table/alias
 * and join predicate; nothing is cached or executed here and it stays outside
 * the RD_* read registry. Each argument re-emits the previously inline SQL
 * byte for byte. */

export const manufacturingStockFootprintSelect = (
  columns: string,
  table: string,
  alias: string,
  on: string,
) => `SELECT ${columns} FROM manufacturing_orders o JOIN ${table} ${alias} ON ${on}`;
