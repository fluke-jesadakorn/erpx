/** Procurement read-tail SQL fragments shared across owner adapters. Text only: binds and locks are unchanged. */
export const workforceActorScopeColumns = `(SELECT ?::text company_id,?::text branch_id,?::text department_id)`;
export const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export const commandReplayGuard = `SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?`;
