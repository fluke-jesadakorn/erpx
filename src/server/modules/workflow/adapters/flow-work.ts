import type { WorkContext } from '../../identity_access/public/authorization';
import type { WorkItem } from "@/lib/work_projection/work-item";
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { holdingResources } from '../../runtime/public/runtime';
import { ApiError } from '../../runtime/public/http';
import { BusinessRuleError } from '../../shared';
import { requireFlowAccess } from '../public/flow-access';
import { resolveFlow } from '../public/flow-routing';
import { policyRead } from '../../identity_access/public/iam-read-snapshot';
import { createFlowWorkApplication } from '../application/flow-work';
import { flowWorkActionKind, workFlowKind } from '../domain/flow-work';
import { workflowReferenceRows as commerceReferences } from '../../commerce/public/workflow-read-sources';
import { workflowReferenceRows as financeReferences } from '../../finance/public/workflow-document-sources';
import { workflowReferenceRows as peopleReferences } from '../../people_payroll/public/workflow-document-sources';
export { flowWorkActionKind, workFlowKind };
function application(provided?: SqlDatabase) {
    let write = provided!;
    return createFlowWorkApplication({ prepare: async (holding) => { if (!provided)
            write = (await holdingResources(holding)).write; }, policyRead: (key, read) => policyRead(write, key, read), resolveFlow: (actor, kind, id) => fromNative(() => resolveFlow(write, actor, kind, id)), requireFlowAccess: (actor, kind, id) => fromNative(async () => { await requireFlowAccess(actor, kind, id, write); }), workRows: (actor, kind, reference) => fromNative(async () => {
            if (reference)
                return (await (kind === 'order' ? commerceReferences(write, kind, actor.companyId) : kind === 'payroll' ? peopleReferences(write, kind, actor.companyId) : financeReferences(write, kind, actor.companyId))).results;
            switch (kind) {
                case 'order': return (await import('../../commerce/public/commerce')).workOrderRows(actor, write);
                case 'invoice': return (await import('../../finance/public/finance')).workInvoiceRows(actor, write);
                case 'journal': return (await import('../../finance/public/ledger')).workJournalRows(actor, write);
                case 'payroll': return (await import('../../people_payroll/public/people-payroll')).workPayrollRows(actor, write);
            }
        }) });
}
async function fromNative<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export function enrichFlowWork(actor: WorkContext, items: WorkItem[]) { return invoke(() => application().enrichFlowWork(actor, items)); }
export function otherFlowWork(actor: WorkContext, reference = false): Promise<WorkItem[]> { return invoke(() => application().otherFlowWork(actor, reference)); }
