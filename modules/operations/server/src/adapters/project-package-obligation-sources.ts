import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {workOperationsSettlementSources} from "./work-package-obligation-sources";
/** Raw included Project task/routine facts, independent of any user's queue.
 * The three owner-local fixed selections reference the canonical Work
 * settlement literals; no duplicate TypeScript SQL remains in this adapter. */
export const projectTaskSettlementSources={tasks:workOperationsSettlementSources.tasks,templates:workOperationsSettlementSources.templates,schedules:workOperationsSettlementSources.schedules} as const;
export function lockProjectTaskSettlementSources(database:SqlDatabase){return database.prepare("LOCK TABLE staff_tasks,routine_templates,routine_schedules IN SHARE MODE").run();}
