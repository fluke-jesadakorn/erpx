import "server-only";
import { legalPointByKeySql } from './legal-read-tail-r36-sources';
/** Byte-identical Legal EXISTS point-guard head family shared by owner adapters (round 40).
 * R66: the read head delegates to the verified r36 arbitrary-key point-read renderer
 * with the fixed '1' projection; only the trusted ` AND ` tail stays local. Every
 * call site still prepares, binds and executes its own assertion with unchanged binds
 * and byte-identical SQL. Callers pass trusted constant table/key identifiers; no
 * request text is interpolated and no bind is introduced. */
export const legalExistsPointGuardHeadSql = (table: string, key: string) =>
  `EXISTS(${legalPointByKeySql('1', table, key as Parameters<typeof legalPointByKeySql>[2])} AND `;
