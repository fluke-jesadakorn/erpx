/** G4 / retire.crm.read-tail-r33 byte-identical SELECT-head family.
 *
 * The workspace/staff/inbox source scope and the lead projection scope share
 * one clock-scoped existence head: both project a read witness from a clocked
 * CRM source join keyed by an id/company pair and an authority tail. The renderer
 * below reproduces the pre-dedupe inline SQL byte-for-byte; callers pass only
 * their clock alias/CTE, source projection, row alias, id/company expressions
 * and predicate tail. Transaction locks, command-receipt reads, native command
 * kinds and the r8-r32 families are untouched.
 */
export const crmClockedSourceScopeSql = (
  clock: string,
  cte: string,
  source: string,
  alias: string,
  id: string,
  company: string,
  tail: string,
) => `EXISTS(WITH ${clock}${cte} SELECT 1 FROM (${source}) ${alias} CROSS JOIN ${clock} WHERE ${alias}.id=${id} AND ${alias}.company_id=${company} AND ${tail}`;
