/** Byte-identical Agent monthly-union count head shared by collaboration-ai-sources
 * (five-branch records quota union) and executive-generation-quota (two-branch
 * request/finance union). Both call sites author the same count-from-union prefix
 * and the same company/month predicate tail; only the union body and the optional
 * `calls` alias vary. Each call site keeps its own binds; the native quota owner
 * remains the authority. */
export const agentMonthlyUnionCountSql = (union: string, alias: '' | 'calls' = ''): string =>
  `(SELECT count(*) FROM (${union})${alias ? ` ${alias}` : ''} WHERE company_id=? AND month=?)`;
