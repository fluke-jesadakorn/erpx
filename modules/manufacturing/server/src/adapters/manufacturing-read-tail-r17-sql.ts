/** Byte-identical Manufacturing read-tail r17 SQL text shared across the
 * package-settlement fact UNION branches. Its projection-from-source head is
 * shared with the purchased-scope reader through the owner-local r37 emitter;
 * every branch keeps its own literal columns, id/projection expressions,
 * source rows and predicate, and nothing is cached or executed here. It stays
 * outside the RD_* read registry. */
import {manufacturingProjectionFromSelect} from "./manufacturing-read-tail-r37-sql";

export const manufacturingSettlementFactSelect = (projection: string, source: string) =>
  manufacturingProjectionFromSelect(projection, source);
