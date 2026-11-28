import "server-only";
import contract from "./legal-read-tail-r42-sources.json" with { type: "json" };
/** Native-owned Legal feature-search read head shared by the feature count and
 * list reads. Callers supply only the fixed projection and trusted fact table;
 * the search predicate and its three binds stay owner-controlled. */
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token))
      throw new Error("LEGAL_READ_TAIL_R42_CONTRACT_INVALID");
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
export const legalFeatureSearchSql = (projection: string, table: string) =>
  fill(contract.featureSearch, { projection, table });
