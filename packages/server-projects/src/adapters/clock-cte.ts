import {packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
/** Byte-identical package clock CTE shared by the Projects lock-free read guards.
 * The named CTE and its MATERIALIZED mode are preserved per call site; only the
 * alias varies, so the emitted SQL matches the former inline copies exactly. */
export const projectsClockCte=(name:string)=>`WITH ${name} AS MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
