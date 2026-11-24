/** Text-only fragments shared by Manufacturing read-tail adapters.
 * Every call site still prepares and executes its own statement with its own
 * binds and transaction; nothing here caches a result or touches the RD_*
 * read registry. Each helper emits the previously inline text byte for byte. */

/** Text-only jsonb_agg subquery emitter shared by the six Manufacturing
 * read-tail aggregate definitions. `where` is the literal trailing clause after
 * the FROM source, including its leading space or comma, so each caller emits
 * its original SQL byte for byte. Nothing is prepared or executed here. */
export const manufacturingJsonbAggSql = (projection: string, order: string, from: string, where = "") =>
  `(SELECT jsonb_agg(${projection} ORDER BY ${order}) FROM ${from}${where})`;

export const manufacturingOperationAggSql = (table: string, predicate: string) =>
  manufacturingJsonbAggSql("to_jsonb(operation)", "operation.id", table, ` ${predicate}`);

export const manufacturingSelectedAssignmentExists = (moduleColumn: string) =>
  `EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array(${moduleColumn})))`;

export const manufacturingSpecificationLineAggSql = (alias: string) =>
  manufacturingJsonbAggSql("to_jsonb(line)", "line.id", "manufacturing_specification_lines line", ` WHERE line.specification_id=${alias}.id`);

export const manufacturingSpecificationProductsAggSql = (alias: string, productSources: string) =>
  manufacturingJsonbAggSql(
    "to_jsonb(product)",
    "product.id",
    `(${productSources}) product`,
    ` WHERE product.company_id=${alias}.company_id AND (product.id=${alias}.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=${alias}.id))`,
  );

export const manufacturingScopedCte = (name: string, source: string, alias: string) =>
  `${name} AS MATERIALIZED(SELECT ${alias}.* FROM ${source} ${alias},scope WHERE ${alias}.company_id=scope.company_id)`;

export const manufacturingActorScopeRows = () =>
  `(SELECT ?::text company_id,?::text branch_id,?::text department_id)`;

export const manufacturingRowsSelect = (table: string, where = "") =>
  `SELECT * FROM ${table}${where ? ` WHERE ${where}` : ""}`;

export const manufacturingIdSelect = (table: string) => `SELECT id FROM ${table}`;

export const manufacturingNativeProductionProbe = (ids: string, where: string) =>
  `SELECT 1 FROM (${ids}) native_production WHERE ${where}`;

/** Text-only attachment target version guard shared by the operational and
 * service fact adapters. Both previously inlined the same EXISTS with only the
 * table and optional trailing predicate varying; this emits their SQL byte for
 * byte. It stays a live statement inside each caller's assertion on the same
 * transaction and is outside the RD_* read registry. */
export const manufacturingAttachmentVersionGuard = (table: string, extra = "") =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ""})`;

/** Text-only attachment target descriptor projection shared by the operational
 * and policy fact adapters. The three callers projected the same
 * company/branch/department/version descriptor with only the expressions,
 * alias style and optional predicate varying; this emits each one's SQL byte
 * for byte. It stays inside each caller's own prepared statement and is
 * outside the RD_* read registry. */
export const manufacturingTargetDescriptorSelect = (
  table: string,
  branch: string,
  department: string,
  opts: { version?: string; asAlias?: boolean; extra?: string } = {},
) => {
  const { version = "version", asAlias = false, extra = "" } = opts;
  const alias = (column: string) => asAlias ? ` AS ${column}` : ` ${column}`;
  return `SELECT company_id,${branch}${alias("branch_id")},${department}${alias("department_id")},${version}${asAlias ? " AS version" : ""} FROM ${table} WHERE id=? AND company_id=?${extra}`;
};
