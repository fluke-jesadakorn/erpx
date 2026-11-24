/** Text-only fragments shared by Manufacturing read-tail adapters.
 * Every call site still prepares and executes its own statement with its own
 * binds and transaction; nothing here caches a result or touches the RD_*
 * read registry. Each helper emits the previously inline text byte for byte. */

export const manufacturingOperationAggSql = (table: string, predicate: string) =>
  `(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM ${table} ${predicate})`;

export const manufacturingSelectedAssignmentExists = (moduleColumn: string) =>
  `EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array(${moduleColumn})))`;

export const manufacturingSpecificationLineAggSql = (alias: string) =>
  `(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM manufacturing_specification_lines line WHERE line.specification_id=${alias}.id)`;

export const manufacturingSpecificationProductsAggSql = (alias: string, productSources: string) =>
  `(SELECT jsonb_agg(to_jsonb(product) ORDER BY product.id) FROM (${productSources}) product WHERE product.company_id=${alias}.company_id AND (product.id=${alias}.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=${alias}.id)))`;

export const manufacturingScopedCte = (name: string, source: string, alias: string) =>
  `${name} AS MATERIALIZED(SELECT ${alias}.* FROM ${source} ${alias},scope WHERE ${alias}.company_id=scope.company_id)`;

export const manufacturingActorScopeRows = () =>
  `(SELECT ?::text company_id,?::text branch_id,?::text department_id)`;
