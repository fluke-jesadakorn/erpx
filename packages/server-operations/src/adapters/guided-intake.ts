import { aiRequestQuotaLock } from "@erpx/server-agent/public/ai-quota";
import "server-only";
import { type GuidedIntakeResult } from "@erpx/feature-shared/lib/ai/guided-intake";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { BetaEnvironment } from "@erpx/server-manage_erpx/public/beta-config";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { workContextGuards } from "@erpx/server-workflow/public/commands";
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";
import { aiSnapshotGuard, aiUsageStatement, resolveAiProvider } from "@erpx/server-agent/public/ai-providers";
import { generateProviderJson, withAiGatewayScope } from "@erpx/server-agent/public/ai-transport";
import { assistantSources } from "@erpx/server-agent/public/workspace-assistant";
import { agentSourceAllowed, assistantGovernanceContext, assistantGovernanceGuards, validateAssistantGovernance, recordAssistantInvocation } from "@erpx/server-agent/public/agent-governance";
import { getWorkCapabilities } from "@erpx/server-work_projection/public/work-capabilities";
import { publicScope } from "@erpx/server-identity_access/public/authorization";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type Input, type Generate } from '@erpx/server-operations/domain/guided-intake';
import { createGuidedIntakeApplication, type GuidedIntakeRepository } from '@erpx/server-operations/application/guided-intake';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { GuidedIntakeWorkflowFacts } from '@erpx/server-workflow/public/operations-guided-intake-facts';
import { GuidedIntakeAgentFacts } from '@erpx/server-agent/public/operations-guided-intake-facts';
import { GuidedIntakeCollaborationFacts } from '@erpx/server-collaboration/public/operations-guided-intake-facts';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: GuidedIntakeRepository<SqlStatement> = { stageCheckAuthorityEffect(...args) { return GuidedIntakeWorkflowFacts(database).stageCheckAuthorityEffect(...args); },
        readPrepareGuidedIntakePrevious(...args) { return GuidedIntakeAgentFacts(database).readPrepareGuidedIntakePrevious(...args); },
        guard3(...args) { return GuidedIntakeAgentFacts(database).guard3(...args); },
        guard4(...args) { return GuidedIntakeCollaborationFacts(database).guard4(...args); },
        stagePrepareGuidedIntakeEffect(...args) { return GuidedIntakeAgentFacts(database).stagePrepareGuidedIntakeEffect(...args); },
        stagePrepareGuidedIntakeEffect2(...args) { return GuidedIntakeWorkflowFacts(database).stagePrepareGuidedIntakeEffect2(...args); },
        stagePrepareGuidedIntakeEffect3(...args) { return GuidedIntakeAgentFacts(database).stagePrepareGuidedIntakeEffect3(...args); },
        stagePrepareGuidedIntakeEffect4(...args) { return GuidedIntakeAgentFacts(database).stagePrepareGuidedIntakeEffect4(...args); } };
    return createGuidedIntakeApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, aiRequestQuotaLock: (...args) => aiRequestQuotaLock(database, ...args), sha256: (...args) => fromNative(() => sha256(...args)), workContextGuards: (...args) => workContextGuards(database, ...args), iamMutationGuards: (...args) => iamMutationGuards(database, ...args), aiSnapshotGuard: (...args) => aiSnapshotGuard(database, ...args), aiUsageStatement: (...args) => aiUsageStatement(database, ...args), resolveAiProvider: (...args) => fromNative(() => resolveAiProvider(database, ...args)), generateProviderJson: (...args) => fromNative(() => generateProviderJson(...args)), withAiGatewayScope: (...args) => withAiGatewayScope(...args), assistantSources: (...args) => fromNative(() => assistantSources(database, ...args)), agentSourceAllowed: (...args) => agentSourceAllowed(...args), assistantGovernanceContext: (...args) => fromNative(() => assistantGovernanceContext(database, ...args)), assistantGovernanceGuards: (...args) => assistantGovernanceGuards(database, ...args), validateAssistantGovernance: (...args) => fromNative(() => validateAssistantGovernance(database, ...args)), recordAssistantInvocation: (...args) => fromNative(() => recordAssistantInvocation(database, ...args)), getWorkCapabilities: (...args) => fromNative(() => getWorkCapabilities(database, ...args)), publicScope: (...args) => publicScope(...args) });
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
export async function guidedIntakeOptions(db: SqlDatabase, actor: WorkContext, env: BetaEnvironment) { return invoke(() => application(db).guidedIntakeOptions(actor, env)); }
export async function prepareGuidedIntake(db: SqlDatabase, actor: WorkContext, env: BetaEnvironment, input: Input, id: string, generate: Generate = generateProviderJson): Promise<GuidedIntakeResult> { return invoke(() => application(db).prepareGuidedIntake(actor, env, input, id, generate)); }
