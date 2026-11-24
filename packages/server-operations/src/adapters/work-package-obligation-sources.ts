import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Canonical plain source read, parameterized by table. Each caller renders the
 * exact same SQL literal; only the table name varies. */
const readAll = (table: string) => `SELECT * FROM ${table}`;
/** Closed actual Work sources. A non-null persisted project_id keeps the
 * Project owner even when a caller has no Project purchase or visible Work. */
export const workOperationsSettlementSources={tasks:readAll('staff_tasks'),templates:readAll('routine_templates'),schedules:readAll('routine_schedules'),pauses:readAll('routine_schedule_pauses'),delegations:readAll('staff_task_delegations'),exceptions:readAll('operations_exceptions'),policies:readAll('operations_queue_policies'),exceptionEvents:readAll('operations_exception_events')} as const;
export function lockWorkOperationsSettlementSources(database:SqlDatabase){return database.prepare('LOCK TABLE staff_tasks,routine_templates,routine_schedules,routine_schedule_pauses,staff_task_delegations,operations_exceptions,operations_queue_policies,operations_exception_events IN SHARE MODE').run();}
