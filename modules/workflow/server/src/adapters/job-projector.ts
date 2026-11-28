import { drainSocialAgent } from "@erpx/collaboration-server/public/social-agent";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { drainRequestAi } from "@erpx/records-server/public/request-ai";
import { registeredResources, registeredDatabases, resourceSlots, type RegistryEnv } from "@erpx/platform-runtime-ui/lib/holding-registry";
import { drainExpenseAi } from "@erpx/people-payroll-server/public/expense-ai";
import type { BetaEnvironment } from "@erpx/manage-erpx-server/public/beta-config";
export type JobEnv = RegistryEnv & BetaEnvironment & {
    [binding: string]: unknown;
    CONTROL_DB?: SqlDatabase;
};
import { createJobProjectorApplication } from '@erpx/workflow-server/application/job-projector';
import type { EventMessage } from '@erpx/workflow-server/domain/job-projector';
import { commitWorkflowProjection } from '@erpx/work-projection-server/public/workflow-projector';
import { taskProjectionSource } from '@erpx/operations-server/public/workflow-projection-source';
import { peopleProjectionSource } from '@erpx/people-payroll-server/public/workflow-projection-source';
import { workflowCustomerNotificationId } from '@erpx/sales-server/public/workflow-notification-source';
export type { EventMessage } from '@erpx/workflow-server/domain/job-projector';
export async function project(env: JobEnv, message: EventMessage) {
    const known = resourceSlots(env).some(slot => slot.id === message.holdingId);
    if (!known)
        throw Error('INVALID_HOLDING');
    const { read, write } = registeredDatabases(env, message.holdingId);
    return createJobProjectorApplication({ knownHolding: () => known, clock: { now: Date.now }, production: process.env.NODE_ENV === 'production',
        customerNotificationId: id => workflowCustomerNotificationId(write, id), deliverCustomerNotification: async (id) => (await import('@erpx/sales-server/public/sales-customer-notifications')).deliverCustomerNotification(write, env, id),
        legalAi: async () => { await (await import('@erpx/legal-server/public/legal/jobs')).drainLegalAi(write, env); }, executiveGeneration: async () => { await (await import('@erpx/platform-runtime-server/public/executive/generation')).drainExecutiveGeneration(write, env); }, socialAgent: async (id) => { await drainSocialAgent(write, env, id, undefined, registeredResources(env, message.holdingId).files, message.holdingId); },
        expenseAi: async (id) => { await drainExpenseAi(write, registeredResources(env, message.holdingId).files, env, id); }, financeAi: async () => { await (await import('@erpx/finance-server/public/finance-ai')).drainFinanceAi(write, env); }, requestAi: async (id) => { await drainRequestAi(write, env, id); },
        ownerSource: (kind, id) => kind === 'task' ? taskProjectionSource(write, id) : peopleProjectionSource(write, kind, id),
        project: (event, payload, version, work, times) => commitWorkflowProjection(read, event, payload, version, work, times),
    }).project(message);
}
