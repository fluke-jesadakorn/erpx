import {selectBodySql} from './read-tail-r45';
/** Parameterized byte-identical select head for ordered JSONB image aggregates.
 * The scheduled task delegation/step images and the Project attachment member
 * image emit this identical projection head. Alias and ordering column are the
 * only degrees of freedom; each rendered statement stays exactly as its former
 * inline literal. No SQL comes from a caller. */
export const orderedImageAggregateHead = (alias: string, column: string) =>
  selectBodySql(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.${column})`);
