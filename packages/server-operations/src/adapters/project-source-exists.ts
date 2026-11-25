import 'server-only';
import {OPERATIONS_EXISTS_HEAD} from './read-tail-r55-sources';

/** Canonical project-source membership EXISTS probe shared by the Operations
 *  executive visibility arms. The projection source, alias and predicate are
 *  the only varying bytes; `aliasSep` renders the space/no-space join after the
 *  source projection and `openSep` the optional gap after `EXISTS(`. Each
 *  rendering is byte-identical to the retired inline literal. Binds and locks
 *  are unchanged. */
export const operationsProjectSourceExists = (
  source: string,
  alias: string,
  predicate: string,
  aliasSep = ' ',
  openSep = '',
) => `${OPERATIONS_EXISTS_HEAD(openSep)}(${source})${aliasSep}${alias} WHERE ${predicate})`;
