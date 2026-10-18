import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Raw native approval history; Operations joins actual staff_task IDs before
 * treating any row as Work. Expense/Leave/other owners are never Work duties. */
export const workApprovalSettlementSources={instances:"SELECT * FROM approval_instances WHERE record_type='task'",steps:'SELECT * FROM approval_steps',decisions:'SELECT * FROM approval_decisions',routineOutbox:"SELECT * FROM outbox_events WHERE event_type='task.generated' AND aggregate_type='staff_task'",routineAudits:"SELECT * FROM audit_events WHERE action='routine.occurrence' AND record_type='staff_task'"} as const;
export function lockWorkApprovalSettlementSources(database:SqlDatabase){return database.prepare('LOCK TABLE approval_instances,approval_steps,approval_decisions,outbox_events IN SHARE MODE').run();}

/** Task commands may write approval steps, so acquire writer leases directly. */
export function workTaskApprovalSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');}
