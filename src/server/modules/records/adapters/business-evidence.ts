import type {SqlStatement} from '../../runtime/public/runtime/types';
import { recordEvidenceFacts } from "../../files/public/records-evidence-facts";
import 'server-only';
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { AttachmentBindingContract } from "@/server/modules/files/public/attachments/types";
import { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import { attachmentPolicyRegistry } from "@/server/modules/files/public/attachments/policy";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { createBusinessEvidenceApplication, type BusinessEvidenceRepository } from '../application/business-evidence';
import { BusinessRuleError } from '../../shared';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: BusinessEvidenceRepository = { readPrepareBusinessEvidenceRows(...args) { return recordEvidenceFacts(database).readPrepareBusinessEvidenceRows(...args); } };
    return createBusinessEvidenceApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, policies: attachmentPolicyRegistry, prepareAttachmentBinding: (...args) => fromNative(() => prepareAttachmentBinding(database, ...args)) });
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
export async function prepareBusinessEvidence(db: SqlDatabase, actor: WorkContext, input: {
    attachmentSetIds: string[];
    targetTypes: readonly string[];
    targetId: string;
    expectedTargetVersion: number;
    commandId: string;
    bindingType: string;
    bindingId: string;
    contexts: AttachmentBindingContract['contexts'];
}) { return invoke(() => application(db).prepareBusinessEvidence(actor, input)); }
