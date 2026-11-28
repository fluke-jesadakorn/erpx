/** Byte-identical Billing read-tail r22 SQL text shared across the executive
 * snapshot watermark revision readers. Text only: every caller still prepares
 * and executes its own statement with its own source, binds and result type;
 * nothing is cached or executed here. The enrollment, invoice, legacy and plan
 * revision subqueries differ only in fields, order, source and filter. r40:
 * the md5 string_agg head is recomposed through the r35 projection head. */
import { billingProjectionSql } from './billing-read-tail-r35-sql';
export function billingWatermarkRevisionSql(fields: string, orderBy: string, from: string, filter: string) {
  return `(${billingProjectionSql(
    `md5(COALESCE(string_agg(jsonb_build_array(${fields})::text,',' ORDER BY ${orderBy}),''))`,
    `FROM ${from} WHERE ${filter}`
  )})`;
}
