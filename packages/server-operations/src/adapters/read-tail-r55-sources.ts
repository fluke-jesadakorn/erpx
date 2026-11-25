import 'server-only';
/** Canonical owner-local EXISTS head for the Operations read tail. The single
 *  varying byte is `openSep`, the optional gap after `EXISTS(`. Every rendering
 *  is byte-identical to the retired inline head; binds and locks stay local to
 *  each caller. */
export const OPERATIONS_EXISTS_HEAD = (openSep = '') => `EXISTS(${openSep}SELECT 1 FROM `;
