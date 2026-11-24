/** Ecommerce read-tail fragments shared across owner adapters. Text only: binds and locks are unchanged. */
export const ecommerceModuleAssignment = (alias = '') => {
 const p = alias ? `${alias}.` : '';
 return `SELECT 1 FROM assignments${alias ? ` ${alias}` : ''} WHERE ${p}id=? AND ${p}company_id=? AND (jsonb_array_length(${p}modules_json::jsonb)=0 OR ${p}modules_json::jsonb @> '["ecommerce"]'::jsonb)`;
};
export const ecommerceScopeRow = (alias: string, branch = false) => branch
 ? `(SELECT ?::text company_id,?::text branch_id,?::text department_id) ${alias}`
 : `(SELECT ?::text company_id) ${alias}`;
