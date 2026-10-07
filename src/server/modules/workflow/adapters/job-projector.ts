import { drainSocialAgent } from "@/server/modules/collaboration/public/social-agent";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { drainRequestAi } from "@/server/modules/records/public/request-ai";
import { registeredResources, registeredDatabases, resourceSlots, type RegistryEnv } from "@/lib/holding-registry";
import { drainExpenseAi } from "@/server/modules/people_payroll/public/expense-ai";
import type { BetaEnvironment } from "@/server/modules/manage_erpx/public/beta-config";
export type JobEnv = RegistryEnv & BetaEnvironment & {
    [binding: string]: unknown;
    CONTROL_DB?: SqlDatabase;
};
import { createJobProjectorApplication } from '../application/job-projector';
import type { EventMessage } from '../domain/job-projector';
import { commitWorkflowProjection } from '../../work_projection/public/workflow-projector';
import { taskProjectionSource } from '../../operations/public/workflow-projection-source';
import { peopleProjectionSource } from '../../people_payroll/public/workflow-projection-source';
import { workflowCustomerNotificationId } from '../../sales/public/workflow-notification-source';
export type { EventMessage } from '../domain/job-projector';
export async function project(env: JobEnv, message: EventMessage) {
    const known = resourceSlots(env).some(slot => slot.id === message.holdingId);
    if (!known)
        throw Error('INVALID_HOLDING');
    const { read, write } = registeredDatabases(env, message.holdingId);
    return createJobProjectorApplication({ knownHolding: () => known, clock: { now: Date.now }, production: process.env.NODE_ENV === 'production',
        pmsEvent: async (event) => (await import('../../pms/public/pms/social-events')).projectPmsSocialEvent(write, event),
        customerNotificationId: id => workflowCustomerNotificationId(write, id), deliverCustomerNotification: async (id) => (await import('../../sales/public/sales-customer-notifications')).deliverCustomerNotification(write, env, id),
        legalAi: async () => { await (await import('../../legal/public/legal/jobs')).drainLegalAi(write, env); }, executiveGeneration: async () => { await (await import('../../runtime/public/executive/generation')).drainExecutiveGeneration(write, env); }, socialAgent: async (id) => { await drainSocialAgent(write, env, id, undefined, registeredResources(env, message.holdingId).files, message.holdingId); },
        expenseAi: async (id) => { await drainExpenseAi(write, registeredResources(env, message.holdingId).files, env, id); }, financeAi: async () => { await (await import('../../finance/public/finance-ai')).drainFinanceAi(write, env); }, requestAi: async (id) => { await drainRequestAi(write, env, id); },
        ownerSource: (kind, id) => kind === 'task' ? taskProjectionSource(write, id) : peopleProjectionSource(write, kind, id),
        project: (event, payload, version, work, times) => commitWorkflowProjection(read, event, payload, version, work, times),
    }).project(message);
}
