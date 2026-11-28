import "server-only";
import { legalPointByIdSql } from './legal-read-tail-r32-sources';
/** Byte-identical Legal version-assertion read fragment shared by owner adapters (round 26).
 * R64: delegates the id-key point head to the verified r32 renderer, keeping the
 * ` AND version=?` tail local. Text only: every call site still prepares and executes
 * its own assertion with unchanged binds. */
/** Optimistic-concurrency version witness; the caller supplies the fixed legal table. */
export const legalVersionExistsSql = (table: string) =>
  `EXISTS(${legalPointByIdSql('1', table)} AND version=?)`;
