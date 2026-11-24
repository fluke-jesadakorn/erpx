import 'server-only';
/** r18 addition: owner-supplied projection column list for the single-id user
 * attribute read. Three callers previously repeated this same statement with
 * only the projection varying; one fixed WHERE-bound statement now serves them
 * so the projection cannot drift. Only an owner-supplied fixed column list is
 * accepted, never caller input. No lock, authorization probe, write or earlier
 * round helper is changed. */
export type UserAttributeProjection = 'id,email' | 'id,name' | 'id,name,email';
export const userAttributeByIdSql = (columns: UserAttributeProjection) =>
  `SELECT ${columns} FROM "user" WHERE id=?`;
