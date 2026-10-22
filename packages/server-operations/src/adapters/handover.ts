import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { readDailyPlan } from "@erpx/server-work_projection/public/daily-plan";
import { handoverTaskAccessSql } from "@erpx/server-operations/public/handover-policy";
import { publishPost, savePostDraft } from "@erpx/server-collaboration/public/social-home";
import { readablePost, sourceGuard } from "@erpx/server-collaboration/public/social-source";
import { staffPeople } from "@erpx/server-operations/public/staff-policy";
import { getTask } from "@erpx/server-operations/public/staff-tasks";
import { readablePmsSource, pmsSourceRecipients } from "@erpx/server-pms/public/pms/source";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkSnapshot } from "@erpx/server-work_projection/public/work-items";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import {handoverPublishSchema, type HandoverView, type HandoverAcknowledgement } from '@erpx/server-operations/domain/handover';
import {withOperationsHandoverPurchaseUnit,withOperationsHandoverAcknowledgement} from '@erpx/server-collaboration/public/work-social-purchases';
import { createHandoverApplication, type HandoverRepository } from '@erpx/server-operations/application/handover';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { HandoverCollaborationFacts } from '@erpx/server-collaboration/public/operations-handover-facts';
import { HandoverPmsFacts } from '@erpx/server-pms/public/operations-handover-facts';
export type { HandoverView } from '@erpx/server-operations/domain/handover';
export { handoverPublishSchema } from '@erpx/server-operations/domain/handover';
export type { HandoverAcknowledgement } from '@erpx/server-operations/domain/handover';
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
export async function publishHandover(db: SqlDatabase, actor: WorkContext, snapshot: WorkSnapshot, raw: unknown, key: string) { const input=handoverPublishSchema.parse(raw);return withOperationsHandoverPurchaseUnit(db,actor,input,key,tx=>invoke(() => application(tx).publishHandover(actor, snapshot, input, key))); }
export async function handoverAcknowledgement(db: SqlDatabase, actor: WorkContext, postId: string): Promise<HandoverAcknowledgement> { return invoke(() => application(db).handoverAcknowledgement(actor, postId)); }
export async function acknowledgeHandover(db: SqlDatabase, actor: WorkContext, postId: string, key: string) { return withOperationsHandoverAcknowledgement(db,actor,postId,key,tx=>invoke(() => application(tx).acknowledgeHandover(actor, postId, key))); }

export async function acknowledgeHandoverVersion(db:SqlDatabase,actor:WorkContext,postId:string,key:string,expectedVersion:number){return withOperationsHandoverAcknowledgement(db,actor,postId,key,tx=>invoke(()=>application(tx).acknowledgeHandoverVersion(actor,postId,key,expectedVersion)));}

/** Explicit receipt-aware publication. Legacy publication keeps its original
 * active-day validation; recovery never promotes an unproven legacy draft. */
export async function publishRecoverableHandover(db:SqlDatabase,actor:WorkContext,snapshot:WorkSnapshot,raw:unknown,key:string){const input=handoverPublishSchema.parse(raw);return withOperationsHandoverPurchaseUnit(db,actor,input,key,async tx=>{const id=`post-post-draft-${key}`,prior=await nativeReplayCommand<{id:string;version:number}>(tx,actor,`${key}-operations`,'work.handover.publish',id,input);if(prior){await readablePost(tx,actor,id);return {id:prior.id,version:prior.version};}if(await tx.prepare('SELECT 1 FROM command_receipts WHERE command_id IN(?,?) UNION ALL SELECT 1 FROM social_post_drafts WHERE id=?').bind(key,`${key}-publish`,`post-draft-${key}`).first())throw new NativeApiError(409,'HANDOVER_RECOVERY_UNPROVEN','This legacy submission has no complete original handover receipt. Open its original history before submitting another.');return invoke(()=>application(tx).publishHandover(actor,snapshot,input,key));},true);}
