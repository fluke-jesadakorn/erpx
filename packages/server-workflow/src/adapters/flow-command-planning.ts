import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { FlowKind, FlowView } from '@erpx/feature-shared/lib/flow';
import { createFlowPlanningApplication } from '@erpx/server-workflow/application/flow-routing';
import { nativeFlowPlanningPorts } from '@erpx/server-workflow/adapters/flow-planning-ports';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function prepareFlowCommand(db: SqlDatabase, actor: StaffActor, key: string, kind: FlowKind, id: string, action: string, input: unknown, version: number): Promise<{
    before: SqlStatement[];
    after: SqlStatement[];
    view: FlowView;
}> { return invoke(() => createFlowPlanningApplication(nativeFlowPlanningPorts(db)).prepareFlowCommand(actor, key, kind, id, action, input, version)); }
