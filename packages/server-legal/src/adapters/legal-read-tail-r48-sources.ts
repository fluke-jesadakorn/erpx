import "server-only";

/** Byte-identical Legal read heads shared by the compound corpus source-clean
 * predicate and the legal_members/legal_settings witness families (round 48).
 * Only the trusted trailing predicate varies; every table, alias, join and bind
 * is fixed. Every call site still prepares, binds and executes its own read. */
export const legalCorpusSourceCleanHead = (predicate: string) =>
  `SELECT 1 FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=? AND ${predicate}`;
export const legalMembersWhereHead = (predicate: string) =>
  `SELECT 1 FROM legal_members WHERE ${predicate}`;
export const legalSettingsWhereHead = (predicate: string) =>
  `SELECT 1 FROM legal_settings WHERE ${predicate}`;
