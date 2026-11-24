import "server-only";
/** Canonical owner-local record-image projection shared by the Maintenance
 * attachment target read and the Work/Plan purchase-source read. Both callers
 * pass only owner-fixed jsonb field fragments and a closed FROM/WHERE source;
 * no request-authored SQL or relation name is accepted. Each composed string
 * stays byte-identical to its pre-dedupe literal, with the sole projection
 * keyword kept in this one definition. */
export const recordImageSql = (fields: string, source: string): string =>
  `SELECT jsonb_build_object(${fields})::text image${source}`;
