import "server-only";

/** Parameterized byte-identical projection head shared by the scoped CTE family, the timeline
 * customer set, the directory/visible-customer reads and the scoped EXISTS rows. Only the
 * columns/source/predicate vary, so emitted SQL stays unchanged; r5-r12 helpers, source locks
 * and cross-owner pins remain authoritative. */
export const scopedProjection = (columns: string, from: string, where: string) =>
  `SELECT ${columns} FROM ${from} WHERE ${where}`;
/** Scoped EXISTS row template. The visibility branches differ only by projected column,
 * relation and alias; r13 composes them through the single scopedProjection head. */
const scopedRow = (column: string, relation: string, alias: string, key: string) =>
  scopedProjection(column, `${relation} ${alias}`, `${alias}.${key}=p.id`);
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
