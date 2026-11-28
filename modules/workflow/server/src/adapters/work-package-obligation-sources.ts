import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {approvalInstancesStarWhereSql,approvalStepsStarSql} from './approval-projection-sources';
/** Byte-identical task-settlement source family: the closed key selects only a
 * fixed table/filter pair; no caller supplies a table or predicate. Read-only. */
const workSettlementSourceSql = (key: 'decisions' | 'routineOutbox' | 'routineAudits'): string => {
  const source = {
    decisions: {table: 'approval_decisions', filter: ''},
    routineOutbox: {table: 'outbox_events', filter: " WHERE event_type='task.generated' AND aggregate_type='staff_task'"},
    routineAudits: {table: 'audit_events', filter: " WHERE action='routine.occurrence' AND record_type='staff_task'"},
  }[key];
  return `SELECT * FROM ${source.table}${source.filter}`;
};
/** Raw native approval history; Operations joins actual staff_task IDs before
 * treating any row as Work. Expense/Leave/other owners are never Work duties. */
export const workApprovalSettlementSources={instances:`${approvalInstancesStarWhereSql}record_type='task'`,steps:approvalStepsStarSql,decisions:workSettlementSourceSql('decisions'),routineOutbox:workSettlementSourceSql('routineOutbox'),routineAudits:workSettlementSourceSql('routineAudits')} as const;
export function lockWorkApprovalSettlementSources(database:SqlDatabase){return database.prepare('LOCK TABLE approval_instances,approval_steps,approval_decisions,outbox_events IN SHARE MODE').run();}

/** Task commands may write approval steps, so acquire writer leases directly. */
export function workTaskApprovalSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');}
