/** Owner-local head for the read-only jsonb image projections. The opening
 *  quote is part of the head; callers pass the remaining projection text
 *  through unchanged so the rendered SQL stays byte-identical. */
export const operationsJsonbImageHead = (tail: string) => `SELECT jsonb_build_object('${tail}`;
