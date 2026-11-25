/** Byte-identical Manufacturing read-tail r37 SQL text shared by the
 * settlement-fact and purchased-scope read heads. Both emitters opened the same
 * projection-from-source head, differing only in the trailing source text; one
 * parameterized emitter renders each one byte for byte. Text only: every caller
 * still prepares and executes its own statement with its own binds, result type
 * and transaction; nothing is cached or executed here and it stays outside the
 * RD_* read registry. The r15-r36 helpers, every lock-bearing and native witness
 * and all cross-owner statements remain untouched. */

export const manufacturingProjectionFromSelect = (projection: string, source: string) =>
  `SELECT ${projection} FROM ${source}`;
