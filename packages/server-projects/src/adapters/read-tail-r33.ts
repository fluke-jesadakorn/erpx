import {selectProjectionFrom} from './read-tail-r40';

/** Byte-identical point-read head shared by the Projects assignment-version,
 * attachment descriptor and attachment owner-version guard reads. Only the
 * closed column list, relation and second key column vary; every caller keeps
 * its own trailing predicates, binds, result type and transaction. No SQL
 * comes from a caller and no lock or spaced JOIN text is emitted here. */
export const projectPointReadHead = (
  columns: string,
  table: string,
  key2: 'user_id' | 'company_id',
) => `${selectProjectionFrom(columns, table)} WHERE id=? AND ${key2}=?`;
