/** Text-only allowed-guard head shared by Manufacturing read-tail authorization
 * guards. The attachment-purchase file-scope witness and the two purchased-
 * command assignment guards opened the same predicate/allowed projection head
 * with only the predicate expression and optional FROM tail varying; one
 * parameterized emitter renders each one byte for byte. Each caller still
 * prepares and executes its own statement with its own binds, result type and
 * transaction; nothing is cached or executed here and it stays outside the
 * RD_* read registry. The r4/r11/r15-r30 helpers and all LOCK TABLE / FOR UPDATE
 * witnesses remain untouched. */

export const manufacturingAllowedSelect = (predicate: string, from = "") =>
  `SELECT ${predicate} allowed${from ? ` ${from}` : ""}`;
