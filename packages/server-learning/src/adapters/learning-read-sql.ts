/** Owner-local byte-identical read SQL shared across Learning adapters. A single
 * text source keeps equivalent actor-scope, clock and course-image fragments
 * from drifting apart. */
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";

export const actorScopeProbe = "(SELECT ?::text company_id,?::text branch_id,?::text department_id";
export const clockProbe = `MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
export const coursesImage = "(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM learning_courses c WHERE c.company_id=?)";
