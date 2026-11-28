/** Byte-identical Commerce settlement line-head text shared by the r25 read-tail
 * sites. Text only: every consumer keeps its own source witness and native read
 * port, with its own binds in its own statement; nothing is cached or hoisted
 * here. `tail` is passed verbatim so each rendered statement keeps its exact
 * bytes, aliases and `?` bind order. */
export const commerceSettlementLineSql = (tail: string) =>
  `SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ${tail}`;
