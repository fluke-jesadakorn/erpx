import {projectsClockCte} from './clock-cte';
/** Byte-identical purchase-source head shared by the Project metric, task and
 * scheduled-alert reads. Clock name, source alias and projection are the only
 * degrees of freedom; each caller keeps its own closed predicates and binds.
 * No SQL comes from a caller. */
export const projectPurchaseSourceHead = (clock: string, alias: string, projection: string) =>
  `${projectsClockCte(clock)} SELECT ${projection} FROM staff_projects ${alias} CROSS JOIN ${clock} WHERE `;
