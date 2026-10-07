import { z } from 'zod';
import { flowKinds, type FlowKind } from '@/lib/flow';
export const recordPermissions: Record<FlowKind, readonly string[]> = {
    expense: [
        "self.requests",
        "expense.read",
        "expense.prepare",
        "expense.approve",
        "expense.pay",
        "spending.approve",
    ],
    leave: ["self.requests", "people.read", "people.manage"],
    staff_task: ["self.tasks", "people.read", "people.manage"],
    request: ["self.requests", "spending.approve"],
    stock_count: ["inventory.read", "inventory.prepare", "inventory.approve"],
    transfer: ["inventory.read", "inventory.transfer"],
    order: [
        "commerce.read",
        "commerce.sales",
        "commerce.purchase",
        "commerce.approve",
    ],
    invoice: ["finance.read", "finance.write", "finance.approve", "finance.pay"],
    journal: ["finance.read", "finance.write", "finance.approve"],
    payroll: ["payroll.read", "payroll.prepare", "payroll.approve"],
    record: ["records.read", "records.write"],
};
export const inspectorInput = z.object({
    userId: z.string(),
    assignmentId: z.string(),
    permission: z.string(),
    workspaceId: z.string().optional(),
    recordType: z.enum(flowKinds).optional(),
    recordId: z.string().optional(),
});
