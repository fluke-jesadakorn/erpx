import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Closed actual Work sources. A non-null persisted project_id keeps the
 * Project owner even when a caller has no Project purchase or visible Work. */
export const workOperationsSettlementSources={tasks:'SELECT * FROM staff_tasks',templates:'SELECT * FROM routine_templates',schedules:'SELECT * FROM routine_schedules',pauses:'SELECT * FROM routine_schedule_pauses',delegations:'SELECT * FROM staff_task_delegations',exceptions:'SELECT * FROM operations_exceptions',policies:'SELECT * FROM operations_queue_policies',exceptionEvents:'SELECT * FROM operations_exception_events'} as const;
export function lockWorkOperationsSettlementSources(database:SqlDatabase){return database.prepare('LOCK TABLE staff_tasks,routine_templates,routine_schedules,routine_schedule_pauses,staff_task_delegations,operations_exceptions,operations_queue_policies,operations_exception_events IN SHARE MODE').run();}
