import { type StaffTaskAlertResult, type TaskAlertRow } from '@erpx/server-operations/domain/staff-task-alerts';
import { type Clock, type IdentifierSource } from '@erpx/server-shared/index';

export interface StaffTaskAlertsRepository {
    readEscalateStaffTasksRows(now: number, now2: number, value: number): Promise<{
        results: (TaskAlertRow)[];
    }>;
    readEscalateStaffTasksApprovers(company_id: string, id: string): Promise<{
        results: ({
            approver_user_id: string;
        })[];
    }>;
    stageEscalateStaffTasksResult(value: string, company_id: string, userId: string, value2: "staff_task_alert", id: string, version: number, message: string, now: number, value3: string[],source?:TaskAlertRow): Promise<{
        success: boolean;
        meta: {
            changes: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
}
export interface StaffTaskAlertsPorts<Effect> {
    repository: StaffTaskAlertsRepository;
    clock: Clock;
    identifiers: IdentifierSource;

    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
}
export function createStaffTaskAlertsApplication<Effect>(ports: StaffTaskAlertsPorts<Effect>) {
    const r = ports.repository;
    async function escalateStaffTasks(now: number, warningWindowMs = 86400000): Promise<StaffTaskAlertResult> {
        const rows = (await r.readEscalateStaffTasksRows(now, now, now + warningWindowMs)).results;
        let dueSoon = 0;
        let overdue = 0;
        for (const task of rows) {
            if (task.due_at === null)
                continue;
            const isOverdue = task.due_at < now;
            const phase = isOverdue ? "overdue" : "due-soon";
            const message = isOverdue
                ? `Task overdue: ${task.title}. Follow up or escalate the owner.`
                : `Task due within 24 hours: ${task.title}.`;
            const recipients = new Set<string>([
                task.assignee_user_id,
                ...(task.delegate_user_id ? [task.delegate_user_id] : []),
                ...(isOverdue ? [task.creator_user_id, task.project_owner_id ?? ""] : []),
            ]);
            const approvers = await r.readEscalateStaffTasksApprovers(task.company_id, task.id);
            for (const approver of approvers.results)
                recipients.add(approver.approver_user_id);
            recipients.delete("");
            for (const userId of recipients) {
                const result = await r.stageEscalateStaffTasksResult(`staff-task-alert-${phase}-${task.id}-${task.version}-${userId}`, task.company_id, userId, "staff_task_alert", task.id, task.version, message, now, (ports.production ? [task.company_id] : []),task);
                if (Number(result.meta.changes) > 0) {
                    if (isOverdue)
                        overdue += 1;
                    else
                        dueSoon += 1;
                }
            }
        }
        return { dueSoon, overdue };
    }
    return { escalateStaffTasks };
}
