import 'server-only';
/** r21 addition: owner-supplied projection column lists for the remaining
 * single-id user attribute reads not covered by r18's closed union (id;
 * name,email; email; id,name,email,"updatedAt"). One fixed WHERE-bound statement
 * serves the four callers so the projection cannot drift. This is a separate,
 * extended union; r18's closed union is unchanged. Only an owner-supplied fixed
 * column list is accepted, never caller input. No lock, authorization probe,
 * write or earlier-round helper is changed. */
export type ExtendedUserAttributeProjection = 'id' | 'name,email' | 'email' | 'id,name,email,"updatedAt"';
export const userAttributeByIdExtendedSql = (columns: ExtendedUserAttributeProjection) =>
  `SELECT ${columns} FROM "user" WHERE id=?`;
