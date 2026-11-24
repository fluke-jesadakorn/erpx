import 'server-only';
import {selectOneProbe} from './customer-authority-read-sql';

/** One canonical fact-witness probe shared by the Customer Authority
 *  package-obligation conclusion and outstanding-facts reads (G4 read-tail
 *  r32). Only the closed owner `EXISTS`/`NOT EXISTS` polarity, source clause and
 *  predicate are supplied; each rendered statement keeps its exact pre-dedupe
 *  SQL bytes and its own prepared statement and binds. This family is a pure
 *  read: no writes, no row/table locks and no advisory locks. The r21-r31
 *  builders, the control-plane lock statement, native-owned fact kinds and the
 *  manager-eligibility helpers stay untouched. */
export const factExistenceWitness = (negated: boolean, source: string, predicate: string) =>
  `${negated ? 'NOT ' : ''}EXISTS(${selectOneProbe(`FROM ${source} WHERE ${predicate}`)})`;
