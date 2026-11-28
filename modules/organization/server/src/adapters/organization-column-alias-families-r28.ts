import 'server-only';
/** r28 additions: relation/alias-parameterized owner read SQL. These families
 * are new; every r17-r27 helper and its call sites are left untouched. Every
 * call renders byte-identical SQL to the literal it replaces; only owner-
 * supplied relations, aliases and reference expressions vary (never caller
 * input). Text only: callers keep their own binds and supplied statements. */

/** Company-bound JSONB snapshot aggregate parameterized by relation and alias.
 * Family: the graph-guard position/level/node images and the project-worker
 * startup node/position/level images. */
export const companySnapshotAggregate = (table: string, alias: string) =>
  `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} WHERE ${alias}.company_id=?)`;

/** Correlated owner-node JSON lookup parameterized by alias and the two
 * reference expressions. Family: the attachment source image branch and
 * target/branch node reads. */
export const correlatedNodeJson = (alias: string, idRef: string, companyRef: string) =>
  `(SELECT to_jsonb(${alias}) FROM organization_nodes ${alias} WHERE ${alias}.id=${idRef} AND ${alias}.company_id=${companyRef})`;
