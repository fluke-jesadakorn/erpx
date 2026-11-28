import 'server-only';

/** Canonical alias-qualified SELECT-all head shared by the Operations read tail.
 *  Alias, table and the WHERE/ORDER tail are the only varying bytes; every
 *  rendering is byte-identical to the retired inline literal. Binds and locks
 *  are unchanged. */
export const operationsAliasSelectAll = (alias: string, table: string, tail: string) =>
  `SELECT ${alias}.* FROM ${table} ${alias} WHERE ${tail}`;
