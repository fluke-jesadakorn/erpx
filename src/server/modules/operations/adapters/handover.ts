import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { readDailyPlan } from "@/server/modules/work_projection/public/daily-plan";
import { handoverTaskAccessSql } from "@/server/modules/operations/public/handover-policy";
import { publishPost, savePostDraft } from "@/server/modules/collaboration/public/social-home";
import { readablePost, sourceGuard } from "@/server/modules/collaboration/public/social-source";
import { staffPeople } from "@/server/modules/operations/public/staff-policy";
import { getTask } from "@/server/modules/operations/public/staff-tasks";
import { readablePmsSource, pmsSourceRecipients } from "@/server/modules/pms/public/pms/source";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { WorkSnapshot } from "@/server/modules/work_projection/public/work-items";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { type HandoverView, type HandoverAcknowledgement } from '../domain/handover';
import { createHandoverApplication, type HandoverRepository } from '../application/handover';
import { BusinessRuleError } from '../../shared';
import type { SqlStatement } from '../../runtime/public/runtime/types';
import { HandoverCollaborationFacts } from '../../collaboration/public/operations-handover-facts';
import { HandoverPmsFacts } from '../../pms/public/operations-handover-facts';
export type { HandoverView } from '../domain/handover';
export { handoverPublishSchema } from '../domain/handover';
export type { HandoverAcknowledgement } from '../domain/handover';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: HandoverRepository<SqlStatement> = { readHandoverViewVersions(...args) { return HandoverCollaborationFacts(database).readHandoverViewVersions(...args); },
        readPublishHandoverExisting(...args) { return HandoverCollaborationFacts(database).readPublishHandoverExisting(...args); },
        readPublishHandoverAccessible(...args) { return HandoverPmsFacts(database).readPublishHandoverAccessible(...args); },
        readPublishHandoverAccessible2(recipientId: string, id: string, companyId: string) {
            return database.prepare(`SELECT 1 FROM staff_tasks ht CROSS JOIN (SELECT ?::text user_id) recipient
        WHERE ht.id=? AND ht.company_id=? AND ${handoverTaskAccessSql("ht", "recipient.user_id")}`)
                .bind(recipientId, id, companyId).first();
        },
        readHandoverAcknowledgementRecipients(...args) { return HandoverCollaborationFacts(database).readHandoverAcknowledgementRecipients(...args); },
        readHandoverAcknowledgementAcknowledgements(...args) { return HandoverCollaborationFacts(database).readHandoverAcknowledgementAcknowledgements(...args); },
        guard7(...args) { return HandoverCollaborationFacts(database).guard7(...args); },
        stageAcknowledgeHandoverEffect(...args) { return HandoverCollaborationFacts(database).stageAcknowledgeHandoverEffect(...args); } };
    return createHandoverApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, readDailyPlan: (...args) => fromNative(() => readDailyPlan(database, ...args)), publishPost: (...args) => fromNative(() => publishPost(database, ...args)), savePostDraft: (...args) => fromNative(() => savePostDraft(database, ...args)), readablePost: (...args) => fromNative(() => readablePost(database, ...args)), sourceGuard: (...args) => sourceGuard(database, ...args), staffPeople: (...args) => fromNative(() => staffPeople(database, ...args)), getTask: (...args) => fromNative(() => getTask(database, ...args)), readablePmsSource: (...args) => fromNative(() => readablePmsSource(database, ...args)), pmsSourceRecipients: (...args) => fromNative(() => pmsSourceRecipients(database, ...args)) });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function handoverView(db: SqlDatabase, actor: WorkContext, snapshot: WorkSnapshot, now = Date.now()): Promise<HandoverView> { return invoke(() => application(db).handoverView(actor, snapshot, now)); }
export async function publishHandover(db: SqlDatabase, actor: WorkContext, snapshot: WorkSnapshot, raw: unknown, key: string) { return invoke(() => application(db).publishHandover(actor, snapshot, raw, key)); }
export async function handoverAcknowledgement(db: SqlDatabase, actor: WorkContext, postId: string): Promise<HandoverAcknowledgement> { return invoke(() => application(db).handoverAcknowledgement(actor, postId)); }
export async function acknowledgeHandover(db: SqlDatabase, actor: WorkContext, postId: string, key: string) { return invoke(() => application(db).acknowledgeHandover(actor, postId, key)); }
