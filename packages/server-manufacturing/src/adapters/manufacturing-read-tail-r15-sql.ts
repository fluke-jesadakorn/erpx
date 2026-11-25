/** Byte-identical Manufacturing read-tail r15 SQL text shared across the
 * owner-local production order readers. Text only: every caller still prepares
 * and executes its own statement with its own projection, optional join,
 * predicate, binds and result type; nothing is cached or executed here and it
 * stays outside the RD_* read registry. The `join` parameter carries the native
 * commerce output-product join verbatim where a caller needs it.
 *
 * r34: the order/specification head now delegates to the shared r23 stock-
 * footprint head, then appends the caller's optional join and WHERE tail. The
 * rendered text is byte-identical to the former inline head at all callers. */
import { manufacturingStockFootprintSelect } from './manufacturing-read-tail-r23-sql';

export const manufacturingOrderJoinSelect = (columns: string, join: string, tail: string) =>
  `${manufacturingStockFootprintSelect(columns, 'manufacturing_specifications', 's', 's.id=o.specification_id')}${join} WHERE ${tail}`;
