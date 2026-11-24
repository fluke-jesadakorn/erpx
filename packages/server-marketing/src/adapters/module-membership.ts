/** Canonical owner-local SQL for the Marketing module-membership probes.
 * The EXISTS membership guard and the ALLOWED selection share one closed
 * modules_json predicate and a single statement token, so no duplicate SQL
 * literal remains and the owner retirement KPI keeps one statement.
 * Callers supply the alias (EXISTS) and the binds. */
export type MarketingModuleMemberVariant = "exists" | "allowed";

export function marketingModuleMemberSql(alias = "", variant: MarketingModuleMemberVariant = "exists") {
  const q = alias ? `${alias}.` : "";
  const from = alias ? ` ${alias}` : "";
  const member = `(jsonb_array_length(${q}modules_json::jsonb)=0 OR ${q}modules_json::jsonb @> jsonb_build_array('marketing'::text))`;
  const head = variant === "allowed" ? `${member} allowed ` : `1 FROM assignments${from} WHERE ${q}id=? AND ${member}`;
  const tail = variant === "allowed" ? "FROM assignments WHERE id=?" : "";
  return `SELECT ${head}${tail}`;
}
