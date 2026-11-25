/** Byte-identical Commerce settings image text shared by the r27 read-tail
 * sites. Text only: every caller still prepares and executes its own statement
 * with its own binds in its own transaction; no result is cached here. `tail`
 * is passed verbatim so each rendered statement keeps its exact bytes, alias
 * and `?` bind order (the two callers use distinct company predicates). */
export const commerceSettingsImageSql = (tail: string) =>
  `SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE ${tail}`;
