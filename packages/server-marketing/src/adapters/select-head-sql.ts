import 'server-only';

/** Owner-local Marketing head renderer (r46). The module-membership probes and
 * the public-form image point reads share one byte-identical selection head;
 * each caller supplies only its own closed-variant head and tail fragments, and
 * every admitted pair emits its original statement byte for byte. Text only —
 * binds, ORDER and FOR UPDATE/FOR SHARE tails stay at the call sites. Native
 * ports, the protected r38/r40/r45 heads and settlement statements are outside
 * this family. */
export const marketingSelectHeadSql = (head: string, tail = ''): string =>
  `SELECT ${head}${tail}`;
