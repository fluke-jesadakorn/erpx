import 'server-only';
/** Byte-identical existence-guard head shared by the POS record-source authority
 * assertion and the POS attachment owner version guard. Each caller keeps its own
 * interpolated source and WHERE tail, so rendered SQL, binds and locks stay
 * byte-identical. */
export const posExistsSourceHeadSql = (): string => 'EXISTS(SELECT 1 FROM ';
