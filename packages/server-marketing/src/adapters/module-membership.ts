/** Canonical owner-local SQL for the Marketing module-membership existence guard.
 * Every purchase/read statement shares this one alias-parameterized predicate,
 * so no duplicate SQL literal remains. Callers supply the table alias and binds. */
export function marketingModuleMemberSql(alias = "") {
  const q = alias ? `${alias}.` : "";
  const from = alias ? ` ${alias}` : "";
  return `SELECT 1 FROM assignments${from} WHERE ${q}id=? AND (jsonb_array_length(${q}modules_json::jsonb)=0 OR ${q}modules_json::jsonb @> jsonb_build_array('marketing'::text))`;
}
