/** Byte-identical Manufacturing read-tail r24 SQL text shared across the
 * purchased-scope readers (metric, workspace record and attachment sources).
 * Text only: it delegates the scoped source rows to the owner's existing text
 * builder and every caller still prepares and executes its own statement with
 * its own projection, alias, trailing predicate, binds and result type; nothing
 * is cached or executed here and it stays outside the RD_* read registry. Each
 * argument re-emits the previously inline SQL byte for byte. */
import {manufacturingPurchaseScopedRows} from "./purchases";
import {manufacturingProjectionFromSelect} from "./manufacturing-read-tail-r37-sql";

export const manufacturingPurchaseScopedSelect = (projection: string, alias: string, tail: string) =>
  manufacturingProjectionFromSelect(projection, `${manufacturingPurchaseScopedRows(alias)} ${tail}`);
