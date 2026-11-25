/** r53: byte-identical read-only attachment source-state projection head shared
 * by the files owner's two source-state witnesses. Text only: every call site
 * still prepares and executes its own statement in its supplied database with
 * its own bind; no SQL text is accepted from a caller and no result is cached
 * here. Advisory locks, row locks, LOCK TABLE, native writes and every r1-r52
 * helper are left untouched. */

/** Shared source-state projection head (19 bytes). The plain witness completes
 * it with ` FROM attachments WHERE id=?` while the legal-hold witness completes
 * it with `,legal_hold FROM attachments WHERE id=?`; each site keeps its own
 * tail and bind, so rendering the head back into either site reproduces the
 * original inline literal byte for byte. */
export const SOURCE_STATE_HEAD_SQL = 'SELECT source_state';
