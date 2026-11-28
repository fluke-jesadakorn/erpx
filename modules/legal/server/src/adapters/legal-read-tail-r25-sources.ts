import "server-only";
import contract from "./legal-read-tail-r25-sources.json" with { type: "json" };

/** Native-owned Legal read fragments shared by owner adapters (round 25).
 * Callers provide only fixed aliases, tables and order columns; every table,
 * join and bind is owner-controlled in the JSON contract. Every call site still
 * prepares and executes its own statement with unchanged binds. */
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token))
      throw new Error("LEGAL_READ_TAIL_R25_CONTRACT_INVALID");
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
/** Edit-member existence witness; the caller chooses the fixed table alias. */
export const legalMemberEditExistsSql = (alias: string) =>
  fill(contract.memberEditExists, { alias });
/** Item-scoped jsonb aggregate projection; alias, table and order column are fixed by the caller. */
export const legalJsonbAggByItemSql = (alias: string, table: string, order = "id") =>
  fill(contract.jsonbAggByItem, { alias, table, order });
