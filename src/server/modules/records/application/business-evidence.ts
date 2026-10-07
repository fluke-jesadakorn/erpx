import type { AttachmentPolicy } from '../../files/public/attachments/types';
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { AttachmentBindingContract } from "@/server/modules/files/public/attachments/types";
import type { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { RecordsCommands } from '../domain/commands';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: infer Args) => infer Result ? (...args: Args) => PortResult<Result, Effect> : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface BusinessEvidenceRepository {
    readPrepareBusinessEvidenceRows(companyId: string, ids: string[]): Promise<{
        results: ({
            target_type: string;
        })[];
    }>;
}
export interface BusinessEvidencePorts<Effect> {
    repository: BusinessEvidenceRepository;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: RecordsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    policies: readonly AttachmentPolicy[];
    prepareAttachmentBinding(...args: Tail<Parameters<typeof prepareAttachmentBinding>>): PortResult<ReturnType<typeof prepareAttachmentBinding>, Effect>;
}
export function createBusinessEvidenceApplication<Effect>(ports: BusinessEvidencePorts<Effect>) {
    const r = ports.repository;
    async function prepareBusinessEvidence(actor: WorkContext, input: {
        attachmentSetIds: string[];
        targetTypes: readonly string[];
        targetId: string;
        expectedTargetVersion: number;
        commandId: string;
        bindingType: string;
        bindingId: string;
        contexts: AttachmentBindingContract['contexts'];
    }) {
        if (!input.attachmentSetIds.length)
            return null;
        const ids = [...new Set(input.attachmentSetIds)], rows = (await r.readPrepareBusinessEvidenceRows(actor.companyId, ids)).results, types = [...new Set(rows.map(row => row.target_type))];
        if (rows.length !== ids.length || types.length !== 1 || !input.targetTypes.includes(types[0]))
            throw new BusinessRuleError(422, 'ATTACHMENT_BINDING_CONTEXT', 'Choose files for this exact owner and action.');
        const contexts = input.contexts.filter(context => ports.policies.some(policy => policy.targetType === types[0] && policy.purposeKey === context.purposeKey && policy.componentKinds.includes(context.componentKind)));
        if (!contexts.length)
            throw new BusinessRuleError(422, 'ATTACHMENT_BINDING_CONTEXT', 'This owner has no registered file context for this action.');
        return ports.prepareAttachmentBinding(actor, { attachmentSetIds: ids, bindingContract: { targetType: types[0], bindingType: input.bindingType, contexts }, targetId: input.targetId, expectedTargetVersion: input.expectedTargetVersion, commandId: input.commandId, bindingId: input.bindingId });
    }
    return { prepareBusinessEvidence };
}
