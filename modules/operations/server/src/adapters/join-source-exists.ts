import 'server-only';
import {OPERATIONS_EXISTS_HEAD} from './read-tail-r55-sources';

/** Canonical two-source JOIN EXISTS probe shared by the Operations cohort,
 *  policy and alert readers. The two projections, aliases, ON predicate and
 *  WHERE predicate are the only varying bytes; `aliasSep` renders the
 *  space/no-space join after each projection and `whereSep` the optional gap
 *  before `WHERE`. Each rendering is byte-identical to the retired inline
 *  literal. Binds and locks are unchanged. */
export const operationsJoinSourceExists = (
  source: string,
  alias: string,
  joinSource: string,
  joinAlias: string,
  on: string,
  predicate: string,
  aliasSep = ' ',
  whereSep = ' ',
) =>
  `${OPERATIONS_EXISTS_HEAD()}(${source})${aliasSep}${alias} JOIN (${joinSource})${aliasSep}${joinAlias} ON ${on}${whereSep}WHERE ${predicate})`;
