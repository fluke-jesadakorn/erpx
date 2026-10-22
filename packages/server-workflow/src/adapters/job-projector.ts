import { drainSocialAgent } from "@erpx/server-collaboration/public/social-agent";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { drainRequestAi } from "@erpx/server-records/public/request-ai";
import { registeredResources, registeredDatabases, resourceSlots, type RegistryEnv } from "@erpx/feature-runtime/lib/holding-registry";
import { drainExpenseAi } from "@erpx/server-people_payroll/public/expense-ai";
import type { BetaEnvironment } from "@erpx/server-manage_erpx/public/beta-config";
export type JobEnv = RegistryEnv & BetaEnvironment & {
    [binding: string]: unknown;
    CONTROL_DB?: SqlDatabase;
};
import { createJobProjectorApplication } from '@erpx/server-workflow/application/job-projector';
import type { EventMessage } from '@erpx/server-workflow/domain/job-projector';
import { commitWorkflowProjection } from '@erpx/server-work_projection/public/workflow-projector';
import { taskProjectionSource } from '@erpx/server-operations/public/workflow-projection-source';
import { peopleProjectionSource } from '@erpx/server-people_payroll/public/workflow-projection-source';
import { workflowCustomerNotificationId } from '@erpx/server-sales/public/workflow-notification-source';
export type { EventMessage } from '@erpx/server-workflow/domain/job-projector';
export async function project(env: JobEnv, message: EventMessage) {
    const known = resourceSlots(env).some(slot => slot.id === message.holdingId);
    if (!known)
        throw Error('INVALID_HOLDING');
    const { read, write } = registeredDatabases(env, message.holdingId);
    return createJobProjectorApplication({ knownHolding: () => known, clock: { now: Date.now }, production: process.env.NODE_ENV === 'production',
        pmsEvent: async (event) => (await import('@erpx/server-pms/public/pms/social-events')).projectPmsSocialEvent(write, event),
        customerNotificationId: id => workflowCustomerNotificationId(write, id), deliverCustomerNotification: async (id) => (await import('@erpx/server-sales/public/sales-customer-notifications')).deliverCustomerNotification(write, env, id),
        legalAi: async () => { await (await import('@erpx/server-legal/public/legal/jobs')).drainLegalAi(write, env); }, executiveGeneration: async () => { await (await import('@erpx/server-runtime/public/executive/generation')).drainExecutiveGeneration(write, env); }, socialAgent: async (id) => { await drainSocialAgent(write, env, id, undefined, registeredResources(env, message.holdingId).files, message.holdingId); },
        expenseAi: async (id) => { await drainExpenseAi(write, registeredResources(env, message.holdingId).files, env, id); }, financeAi: async () => { await (await import('@erpx/server-finance/public/finance-ai')).drainFinanceAi(write, env); }, requestAi: async (id) => { await drainRequestAi(write, env, id); },
        ownerSource: (kind, id) => kind === 'task' ? taskProjectionSource(write, id) : peopleProjectionSource(write, kind, id),
        project: (event, payload, version, work, times) => commitWorkflowProjection(read, event, payload, version, work, times),
    }).project(message);
}
