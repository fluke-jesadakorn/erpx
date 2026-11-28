/** Procurement read-tail SQL fragments shared across owner adapters. Text only: binds and locks are unchanged. */
export const workforceActorScopeColumns = `(SELECT ?::text company_id,?::text branch_id,?::text department_id)`;
export const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export const commandReplayGuard = `SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?`;
/** Full company/branch/department scope. Bind order stays company, branch, branch, department, department. */
export const branchDepartmentScope = (alias: string) =>
  `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`;
/** Point read keyed by id then company; the tail carries each read's intentional extra predicates. */
export const pointRead = (columns: string, source: string, tail = "") =>
  `SELECT ${columns} FROM ${source} WHERE id=? AND company_id=?${tail}`;
/** Ordered detail read; the order fragment preserves each read's version/ordinal ordering and LIMIT drift. */
export const orderedRead = (columns: string, source: string, where: string, order: string) =>
  `SELECT ${columns} FROM ${source} WHERE ${where} ORDER BY ${order}`;
