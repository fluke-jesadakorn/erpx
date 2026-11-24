/** Text-only fragments shared by Manufacturing read-tail adapters.
 * Every call site still prepares and executes its own statement with its own
 * binds and transaction; nothing here caches a result or touches the RD_*
 * read registry. Each helper emits the previously inline text byte for byte. */

export const manufacturingOperationAggSql = (table: string, predicate: string) =>
  `(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM ${table} ${predicate})`;

export const manufacturingSelectedAssignmentExists = (moduleColumn: string) =>
  `EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array(${moduleColumn})))`;
