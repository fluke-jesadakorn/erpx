import "server-only";
import {legalMembersWhereHead} from './legal-read-tail-r48-sources';
import { legalPointByKeySql } from './legal-read-tail-r36-sources';
import contract from "./legal-read-tail-r38-sources.json" with { type: "json" };
/** Byte-identical Legal read fragment families shared by owner adapters (round 38).
 * Text only: every call site still prepares, binds and executes its own read with unchanged binds.
 * Callers pass trusted constant columns/tables/aliases/keys; no request text is interpolated.
 * R67: the released-item point-guard head delegates to the verified r36 arbitrary-key
 * point-read renderer with the fixed '1' projection and the trusted alias-qualified
 * 'i.id' key; only the trusted ` AND i.<key>=?` tail stays local. No bind is introduced
 * and each rendered statement stays byte-identical to the former inline head.
 * R38 native contract: the fact-data read and metric-count prefix are imported from
 * the owner JSON contract; only trusted table names, aliases and fixed item
 * references are substituted. */
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token))
      throw new Error("LEGAL_READ_TAIL_R38_CONTRACT_INVALID");
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
/** Released-item point guard; the caller supplies the fixed Company/version key. */
export const legalReleasedItemGuardSql = (key: 'company_id' | 'version') =>
  legalPointByKeySql('1', 'legal_items i', 'i.id' as Parameters<typeof legalPointByKeySql>[2]) +
  ` AND i.${key}=?`;
/** Member edit point witness; the caller supplies a bound placeholder or a fixed source alias. */
export const legalMemberEditPointSql = (item: string) =>
  legalMembersWhereHead(`item_id=${item} AND user_id=? AND access='edit'`);
/** Fact-table data read; the caller supplies the trusted fact table and fixed item reference. */
export const legalFactDataSql = (table: string, ref: string) =>
  fill(contract.factData, { table, ref });
/** Metric count projection prefix; the caller supplies the label expression and counted id. */
export const legalMetricCountPrefix = (label: string, id: string) =>
  fill(contract.metricCountPrefix, { label, id });
