import {requireLegacyFinanceInvoiceFlow} from '@erpx/finance-server/public/document-purchases';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { StaffActor } from '@erpx/operations-server/public/staff-policy';
import type { FlowKind, FlowView } from '@erpx/platform-shared-ui/lib/flow';
import { createFlowPlanningApplication } from '@erpx/workflow-server/application/flow-routing';
import { nativeFlowPlanningPorts } from '@erpx/workflow-server/adapters/flow-planning-ports';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { ApiError as NativeApiError } from '@erpx/platform-runtime-server/public/http';
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
}> { if(kind==='invoice')await requireLegacyFinanceInvoiceFlow(db,actor,id);return invoke(() => createFlowPlanningApplication(nativeFlowPlanningPorts(db)).prepareFlowCommand(actor, key, kind, id, action, input, version)); }
