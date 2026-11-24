import "server-only";

/** Parameterized scoped-row template. The visibility EXISTS branches differ only
 * by projected column, relation and alias; one template keeps their SQL byte-identical. */
const scopedRow = (column: string, relation: string, alias: string, key: string) =>
  `SELECT ${column} FROM ${relation} ${alias} WHERE ${alias}.${key}=p.id`;
const existsScoped = (relation: string, alias: string, key: string) =>
  `EXISTS(${scopedRow('1', relation, alias, key)})`;

/** Fixed customer-visibility predicate shared by the directory and timeline
 * visible-customer reads. Each caller keeps its own binding and scope; only the
 * module/permission gates are passed in. */
export function customerVisiblePredicate(
  serviceVisible: boolean,
  financeVisible: boolean,
  legalVisible: boolean,
) {
  return `(${existsScoped('scoped_orders', 'o', 'party_id')} OR ${existsScoped('scoped_leads', 'l', 'customer_id')} OR ${serviceVisible ? existsScoped('scoped_service', 'g', 'customer_id') : 'FALSE'} OR ${financeVisible ? existsScoped('scoped_invoices', 'i', 'party_id') : 'FALSE'} OR ${legalVisible ? `EXISTS(SELECT 1 FROM scoped_legal lg WHERE lg.order_id IN (${scopedRow('id', 'scoped_orders', 'so', 'party_id')}))` : 'FALSE'})`;
}

/** One byte-identical one-value company scope source shared by the portal release
 * projection and the package-obligation facts CTE. Only the caller's alias varies;
 * the emitted one-value company source and its single bind stay identical. */
export const companyScopeSource = (alias: string) =>
  `(SELECT ?::text company_id) ${alias}`.trimEnd();
