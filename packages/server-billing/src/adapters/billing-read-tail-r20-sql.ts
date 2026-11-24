/** Byte-identical Billing read-tail r20 SQL text shared across owner-local
 * usage-detail readers. Text only: every caller still prepares and executes its
 * own statement with its own source, binds and result-type alias; nothing is
 * cached or executed here. */
import {billingPointReadSql} from './billing-read-tail-r29-sql';

/** Usage-meter detail reads share the native commercial meter source and a
 * fixed holding/month predicate; each caller selects its own projection. */
export function billingUsageMeterSql(columns: string, source: string) {
  return billingPointReadSql(`${columns}\n    `, `(${source}) billing_usage_meters`, 'holding_id=$1 AND month=$2 ORDER BY source_key');
}

/** Seat-usage interval reads share the native commercial seat source; each
 * caller supplies its own projection and trailing predicate. The head owns the
 * WHERE keyword; the caller's original leading whitespace stays byte-identical. */
export function billingSeatUsageSql(columns: string, source: string, tail: string) {
  const pivot = /^(\s*)WHERE /.exec(tail)!;
  return billingPointReadSql(columns, `(${source}) billing_seat_usage${pivot[1].slice(0, -1)}`, tail.slice(pivot[0].length));
}
