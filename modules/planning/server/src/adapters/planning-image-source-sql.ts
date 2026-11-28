import "server-only";
import { fillPlanningReadTail, planningReadTailSources } from './planning-read-tail-contract';
/** Owner-local parameterized source-row head shared by the Planning read tail.
 * Every argument is a fixed server-owned column, projection, alias or FROM
 * fragment; no bind value or caller SQL is interpolated. `lead` and `tail` carry
 * the fixed whitespace around the projected row so an emitted source stays
 * byte-identical to the former inline literal it replaces. The exact template is
 * owned by the native read-tail contract and bound into Rust with include_str!. */
export const planningSourceRows = (
  columns: string,
  fragment: string,
  alias: string,
  from: string,
  lead = ',\n ',
  tail = '\n FROM ',
) => fillPlanningReadTail(planningReadTailSources.sourceRows, { columns, lead, fragment, alias, tail, from });
/** r15 image-source head kept as a thin wrapper; the native contract above
 * owns the single read head and emits the identical image projection. */
export const planningImageSourceRows = (columns: string, image: string, from: string) =>
  planningSourceRows(columns, image, 'image', from);
