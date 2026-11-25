import {packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';

/** G4 / retire.projects.read-tail-r41 closed clock projection head.
 *
 * The Projects metric purchase-source clock CTE (`projectsClockCte`, lock-free
 * read guards) and the collaboration-home audience clock CTE both rendered this
 * identical head literal. Only the enclosing `WITH <name> AS MATERIALIZED (` /
 * `)` frame stays at each caller; the clock expression is the same closed
 * constant at both former sites. No caller SQL enters this helper and no write
 * or lock text is emitted, so each rendered CTE stays byte-identical. */
export const clockNowMsHead = () => `SELECT ${packageClockSql} now_ms`;
