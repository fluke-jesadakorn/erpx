import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Raw included Project task/routine facts, independent of any user's queue. */
export const projectTaskSettlementSources={tasks:"SELECT * FROM staff_tasks",templates:"SELECT * FROM routine_templates",schedules:"SELECT * FROM routine_schedules"} as const;
export function lockProjectTaskSettlementSources(database:SqlDatabase){return database.prepare("LOCK TABLE staff_tasks,routine_templates,routine_schedules IN SHARE MODE").run();}
