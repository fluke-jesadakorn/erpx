/** Byte-identical Commerce order-head text shared by the r21 read-tail sites.
 * Text only: every caller still prepares and executes its own statement with its
 * own binds in its own transaction; no result is cached here. `columns` and
 * `tail` are passed verbatim so each rendered statement keeps its exact bytes,
 * aliases and `?` bind order. */
export const commerceOrdersReadSql = (columns: string, tail: string) =>
  `SELECT ${columns} FROM commerce_orders WHERE ${tail}`;
