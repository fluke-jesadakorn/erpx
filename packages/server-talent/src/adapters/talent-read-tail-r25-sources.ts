import "server-only";
/** Byte-identical Talent read-tail r25 SQL text shared by owner source-image
 * reads (round 25). Text only: every caller keeps its own projected row shape,
 * FROM source, predicates and binds; nothing is cached or executed here and
 * the emitted SQL is byte-identical to the former inline copies. */
/** Owner source-image projection; the caller chooses the projected row shape. */
export const talentSourceImageSelect = (projection: string) => `SELECT ${projection}::text image`;
