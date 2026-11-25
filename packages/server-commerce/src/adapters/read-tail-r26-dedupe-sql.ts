/** Byte-identical Commerce order-source image text shared by the r26 read-tail
 * sites. Text only: every caller still prepares and executes its own statement
 * with its own binds in its own transaction; no result is cached here. `tail`
 * is passed verbatim so each rendered statement keeps its exact bytes, aliases
 * and `?` bind order. */
export const commerceOrderSourceImageSql = (tail: string) =>
  `SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE ${tail}`;
