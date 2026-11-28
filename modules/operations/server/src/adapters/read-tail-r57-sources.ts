import 'server-only';
/** Canonical owner-local explicit-column projection head for the Operations read
 *  tail. Both retired owner projections opened with these exact bytes; callers
 *  append their own unchanged column list, source and alias so rendered SQL and
 *  binds stay byte-identical. No locks or viewer-clock state is involved. */
export const OPERATIONS_OWNER_PROJECTION_ID_HEAD = 'SELECT "id","company_id",';
