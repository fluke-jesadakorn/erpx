import { lifecycleInput, type LifecycleInput } from "@erpx/feature-people_payroll/lib/people-lifecycle";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { PayrollStatutoryCommands } from '@erpx/server-people_payroll/application/payroll-statutory';
import type { previewLifecycle, saveLifecycle } from '@erpx/server-people_payroll/public/people-lifecycle';
import { type PlacementBatchInput, type Prepared, type BatchItemPreview } from '@erpx/server-people_payroll/domain/people-placement-batch';
export interface PeoplePlacementBatchRepository {
    currentEmployees(companyId: string): Promise<{
        results: ({
            engagement_id: string;
            person_id: string;
            status: string;
            position_id: string | null;
        })[];
    }>;
    activePositions(companyId: string): Promise<{
        results: ({
            id: string;
        })[];
    }>;
}
export interface PeoplePlacementBatchPorts<Effect> {
    repository: PeoplePlacementBatchRepository;
    authorize(actor: WorkContext, permission: string): WorkContext;
    hash(value: string): string;
    commands: PayrollStatutoryCommands<Effect>;
    preview(actor: WorkContext, input: LifecycleInput, isPlacement: boolean): ReturnType<typeof previewLifecycle>;
    save(actor: WorkContext, input: LifecycleInput, key: string, isPlacement: boolean, token: string, isBatch: boolean, supersede: Parameters<typeof saveLifecycle>[7]): ReturnType<typeof saveLifecycle>;
    transaction<Result>(run: (ports: PeoplePlacementBatchPorts<Effect>) => Promise<Result>): Promise<Result>;
}
export function createPeoplePlacementBatchApplication<Effect>(ports: PeoplePlacementBatchPorts<Effect>) {
    const repository = ports.repository;
    function authority(actor: WorkContext) {
        ports.authorize(actor, "people.manage");
        ports.authorize(actor, "organization.manage");
        if (actor.scope.kind !== "holding" || actor.assignmentScope !== "holding" || actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_AUTHORITY_REQUIRED", "Open the Company workspace with HR authority to move employees.");
    }
    async function prepare(actor: WorkContext, batch: PlacementBatchInput): Promise<Prepared[]> {
        authority(actor);
        const people = (await repository.currentEmployees(actor.companyId)).results;
        const positions = (await repository.activePositions(actor.companyId)).results;
        return batch.items.map(item => {
            const person = people.find(value => value.engagement_id === item.engagementId);
            const position = positions.find(value => value.id === item.positionId);
            let error: string | undefined;
            if (!person || person.status === "ended")
                error = "This employee is no longer current in this Company.";
            else if (!position)
                error = "Choose an active position in this Company.";
            else if (person.position_id === item.positionId)
                error = "This employee already holds this position.";
            if (error || !person)
                return { engagementId: item.engagementId, error };
            return { engagementId: item.engagementId, input: lifecycleInput.parse({
                    expectedVersion: 0,
                    kind: person.status === "onboarding" ? "onboarding" : "transfer",
                    personId: person.person_id,
                    engagementId: item.engagementId,
                    positionId: item.positionId,
                    managerPersonId: item.managerPersonId,
                    acceptRecommendedPay: false,
                    acceptRecommendedLeave: item.acceptRecommendedLeave,
                    effectiveAt: batch.effectiveAt,
                    timezone: batch.timezone,
                    reason: batch.reason,
                }) };
        });
    }
    async function previewPrepared(actor: WorkContext, rows: Prepared[]): Promise<BatchItemPreview[]> {
        const output: BatchItemPreview[] = [];
        for (const row of rows) {
            if (!row.input) {
                output.push({ engagementId: row.engagementId, ready: false, blockers: [row.error ?? "This employee changed."] });
                continue;
            }
            try {
                const preview = await ports.preview(actor, row.input, true);
                output.push({ engagementId: row.engagementId, ready: preview.ready, blockers: preview.blockers, token: preview.token, legacyCaseToSupersede: preview.legacyCaseToSupersede, affectedPeople: preview.affectedPeople.length });
            }
            catch (error) {
                if (!(error instanceof BusinessRuleError))
                    throw error;
                output.push({ engagementId: row.engagementId, ready: false, blockers: [error.message] });
            }
        }
        return output;
    }
    function batchToken(input: PlacementBatchInput, items: BatchItemPreview[]) {
        return ports.hash(JSON.stringify({ input, tokens: items.map(item => item.token) }));
    }
    async function previewPlacementBatch(actor: WorkContext, input: PlacementBatchInput) {
        const items = await previewPrepared(actor, await prepare(actor, input));
        for (const item of items) {
            const acknowledged = input.items.find(value => value.engagementId === item.engagementId)?.supersedeLegacyCase;
            if (item.legacyCaseToSupersede && (acknowledged?.id !== item.legacyCaseToSupersede.id || acknowledged?.expectedVersion !== item.legacyCaseToSupersede.version)) {
                item.ready = false;
                item.blockers.push("Confirm the older request before scheduling this employee.");
            }
        }
        const ready = items.every(item => item.ready);
        return { ready, items, token: ready ? batchToken(input, items) : null };
    }
    async function submitPlacementBatch(actor: WorkContext, input: PlacementBatchInput, token: string, key: string) {
        authority(actor);
        if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
            throw new BusinessRuleError(422, "COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key.");
        const recordId = `placement-batch-${key}`;
        const payload = { input, token };
        const operation = "people.lifecycle.batch.submit";
        const prior = await ports.commands.replay<{
            ids: string[];
            count: number;
        }>(actor, key, operation, recordId, payload);
        if (prior)
            return prior;
        try {
            return await ports.transaction(context => createPeoplePlacementBatchApplication(context).submitPrepared(actor, input, token, key, recordId, payload, operation));
        }
        catch (error) {
            if (error instanceof BusinessRuleError)
                throw error;
            throw new BusinessRuleError(409, "BATCH_CHANGED", "The batch could not be submitted. No placements were saved. Refresh and review again.");
        }
    }
    async function submitPrepared(actor: WorkContext, input: PlacementBatchInput, token: string, key: string, recordId: string, payload: {
        input: PlacementBatchInput;
        token: string;
    }, operation: string) {
        const rows = await prepare(actor, input);
        const items = await previewPrepared(actor, rows);
        if (items.some((item, index) => !item.ready || item.legacyCaseToSupersede && (input.items[index].supersedeLegacyCase?.id !== item.legacyCaseToSupersede.id || input.items[index].supersedeLegacyCase?.expectedVersion !== item.legacyCaseToSupersede.version)) || token !== batchToken(input, items))
            throw new BusinessRuleError(409, "BATCH_PREVIEW_CHANGED", "An employee or destination changed. Review the entire batch again.");
        const ids: string[] = [];
        for (const [index, row] of rows.entries()) {
            const item = input.items[index];
            const preview = items[index];
            if (!row.input || !preview.token)
                throw new BusinessRuleError(409, "BATCH_PREVIEW_CHANGED", "Review the entire batch again.");
            const childKey = `placement-batch:${ports.hash(key).slice(0, 32)}:${index}`;
            const result = await ports.save(actor, row.input, childKey, true, preview.token, true, item.supersedeLegacyCase);
            ids.push(String(result.id));
        }
        const result = { ids, count: ids.length };
        await ports.commands.commit(actor, key, operation, payload, recordId, 1, result, []);
        return result;
    }
    return { previewPlacementBatch, submitPlacementBatch, submitPrepared };
}
