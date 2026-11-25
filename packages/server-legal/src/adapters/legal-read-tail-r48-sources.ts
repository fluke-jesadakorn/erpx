import "server-only";
import contract from "./legal-read-tail-r48-sources.json" with { type: "json" };

/** Native-owned Legal read heads shared by the compound corpus source-clean
 * predicate and the legal_members/legal_settings witness families (round 48).
 * Only the trusted trailing predicate is substituted; every table, alias, join
 * and fixed bind is owner-controlled in the JSON contract. Every call site
 * still prepares, binds and executes its own read. */
const fill = (template: string, predicate: string) => {
  if (!template.includes("{predicate}"))
    throw new Error("LEGAL_READ_TAIL_R48_CONTRACT_INVALID");
  return template.replace("{predicate}", () => predicate);
};
export const legalCorpusSourceCleanHead = (predicate: string) =>
  fill(contract.corpusSourceClean, predicate);
export const legalMembersWhereHead = (predicate: string) =>
  fill(contract.membersWhere, predicate);
export const legalSettingsWhereHead = (predicate: string) =>
  fill(contract.settingsWhere, predicate);
