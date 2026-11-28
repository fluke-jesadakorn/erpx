/** Byte-identical Finance read-tail SQL shared across status/version guards and
 * company-scoped jsonb_agg projections. Text only: every call site keeps its own
 * EXISTS/COALESCE wrapper, prepares the rendered text and binds with its own values.
 * Lock-taking (FOR UPDATE / FOR SHARE) variants stay inline. */

/** Full status/version guard; the EXISTS wrapper and binds remain at the call site. */
export const idCompanyStatusVersionGuard = (table: string, status: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND status='${status}' AND version=?)`;

/** Company-scoped jsonb_agg projection; COALESCE, where present, stays at the call site.
 * `order` is a comma-separated column list qualified by `a`; `extra` appends predicates
 * after the company bind; `from` and `companyAlias` cover JOIN/aliased-company variants. */
export const aggByCompany = (
  a: string,
  table: string,
  order: string,
  extra = '',
  from?: string,
  companyAlias = a,
) =>
  `(SELECT jsonb_agg(to_jsonb(${a}) ORDER BY ${order.split(',').map(c => `${a}.${c.trim()}`).join(',')}) FROM ${from ?? `${table} ${a}`} WHERE ${companyAlias}.company_id=?${extra})`;
