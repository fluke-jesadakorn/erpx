import { evidenceHead } from "./inspection-evidence-source-sql";
/** Owner-local parameterized statement head shared by the Quality hold-source
 * return anchors and the obligation-fact UNION arms. The head routes through
 * evidenceHead so this family keeps no inline SELECT; the projection, source
 * relation and optional predicate are fixed owner-owned fragments supplied by
 * callers and are emitted byte-identically to the former inline copies. No
 * arm carries a bind and no read here acquires a lock. */
export const qualitySourceArmHead = (
  columns: string,
  source: string,
  predicate = '',
) => evidenceHead(columns, ` FROM ${source}`, predicate);
