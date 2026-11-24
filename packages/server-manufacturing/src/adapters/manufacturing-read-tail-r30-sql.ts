/** Text-only scope CTE shared by Manufacturing read-tail callers. Both the
 * package-obligation facts query and the posting source-state query opened the
 * same `WITH scope AS(...)` head with only the optional boolean `full` column
 * varying; one parameterized emitter renders each one byte for byte. Each
 * caller still prepares and executes its own statement with its own binds,
 * result type and transaction; nothing is cached or executed here and it stays
 * outside the RD_* read registry. The r4/r11/r15-r29 helpers and all LOCK TABLE
 * / FOR UPDATE witnesses remain untouched. */

export const manufacturingScopeCte = (withFull: boolean) =>
  `WITH scope AS(SELECT ?::text company_id${withFull ? ",?::boolean full" : ""})`;
