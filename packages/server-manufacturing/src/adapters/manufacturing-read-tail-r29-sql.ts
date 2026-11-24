/** Byte-identical Manufacturing read-tail r29 SQL text shared by the
 * subquery-scoped EXISTS probes. The cost-evidence purchased-production probe
 * and the existing-posting lineage probe inline the same EXISTS-over-subquery
 * head with only the source rows, alias and predicate varying; neither carries
 * a bind of its own. One parameterized emitter renders both byte for byte.
 * Text only: every caller still prepares and executes its own statement with
 * its own binds, result type and transaction; nothing is cached or executed
 * here and it stays outside the RD_* read registry. The r4/r11/r15-r28 helpers
 * and all LOCK TABLE / FOR UPDATE witnesses remain untouched. */

export const manufacturingScopedExistsProbe = (source: string, alias: string, predicate: string) =>
  `EXISTS(SELECT 1 FROM (${source}) ${alias} WHERE ${predicate})`;
