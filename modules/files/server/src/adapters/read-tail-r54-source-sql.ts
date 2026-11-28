/** r54: byte-identical read-only publication-id projection head shared by the
 * files owner's two reconciliation readers. Text only: each call site still
 * prepares and executes its own statement in its supplied database and keeps
 * its own tail; no SQL text is accepted from a caller and no result is cached
 * here. Advisory locks, row locks, LOCK TABLE, native writes and every r1-r53
 * helper are left untouched. */

/** Shared publication-id projection head (21 bytes). The dangling-publication
 * witness completes it with ` FROM attachment_publications publication ...`
 * while the publication-derivative witness completes it with
 * `,publication.company_id,...`; each site keeps its own tail, so rendering the
 * head back into either site reproduces the original inline literal byte for
 * byte. This head carries no FOR SHARE / FOR UPDATE clause. */
export const PUBLICATION_ID_HEAD_SQL = 'SELECT publication.id';
