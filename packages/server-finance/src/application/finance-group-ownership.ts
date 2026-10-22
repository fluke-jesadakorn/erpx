import { z } from "zod";
import { decimalToUnits, unitsToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { halfAway } from "@erpx/server-finance/domain/finance-instrument-policy";
import { calculateFinance } from "@erpx/server-finance/domain/finance-calculations";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { requireFinanceTool } from "@erpx/server-finance/public/finance-company-tools";
import type { requireFinanceMoneyScale } from "@erpx/server-finance/public/finance-money-policy";
import type { sha256 } from "@erpx/server-runtime/public/crypto";
import { type Row, ownershipEditionSchema, acquisitionEditionSchema, groupEvidenceDecisionSchema } from '@erpx/server-finance/domain/finance-group-ownership';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { GroupCommands } from '@erpx/server-finance/domain/group-command-ports';
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
export interface FinanceGroupOwnershipRepository<Effect> {
    readGetRow(id: string, holdingId: string, companyId: string, table: "finance_ownership_interests" | "finance_acquisitions"): Promise<Row | null>;
    readOwnershipSourceEntities(holdingId: string, investorCompanyId: string, investeeCompanyId: string): Promise<{
        results: (Row)[];
    }>;
    readOwnershipSourceOverlap(holdingId: string, id: string | number | null, id2: string | number | null, value: string | number, value2: string, effectiveFrom: string): Promise<Record<string, unknown> | null>;
    stagePrepareOwnershipEditionEffect(id: string, holdingId: string, companyId: string, value: string | null, id2: string | number | null, id3: string | number | null, method: "subsidiary" | "equity" | "cost" | "excluded", ownershipBps: number, votingBps: number, controlAssessment: string, effectiveFrom: string, value2: string | null, userId: string, now: number, now2: number, hash: string, value3: string, reason: string): Effect;
    stageDecideOwnershipEditionEffect(end: string, now: number, id: string | number | null, version: string | number | null): Effect;
    guard6(key: string, value: "ownership-version", id: string, holdingId: string, companyId: string, expectedVersion: number): Effect;
    stageDecideOwnershipEditionEffect2(value: "approved" | "rejected", userId: string, now: number, id: string): Effect;
    readAcquisitionSourceEffect(companyId: string, ownershipId: string, acquisitionDate: string, currency: string, value: string | number, currentId: string): Promise<Record<string, unknown> | null>;
    stagePrepareAcquisitionEditionEffect(calcId: string, companyId: string, holdingId: string, currency: string, calcHash: string, value: string, value2: string, value3: string, userId: string, now: number, now2: number): Effect;
    stagePrepareAcquisitionEditionEffect2(value: string, calcId: string, userId: string, reason: string, value2: string, now: number): Effect;
    stagePrepareAcquisitionEditionEffect3(id: string, holdingId: string, companyId: string, ownershipId: string, investee_entity_id: string | number | null, acquisitionDate: string, currency: string, value: string, value2: string, nciMeasurement: "fair_value" | "proportionate", value3: string, value4: string, value5: string, value6: string, value7: string, cguReference: string, calcId: string, userId: string, now: number, now2: number, hash: string, value8: string, reason: string, value9: string | null, value10: number | null): Effect;
    readDecideAcquisitionEditionCalc(calculation_run_id: string | number | null, companyId: string, holdingId: string): Promise<Row | null>;
    guard13(key: string, value: "acquisition-parent", id: string | number | null, companyId: string, holdingId: string, value2: number): Effect;
    stageDecideAcquisitionEditionEffect(now: number, id: string | number | null): Effect;
    guard15(key: string, value: "acquisition-version", id: string, companyId: string, holdingId: string, expectedVersion: number): Effect;
    stageDecideAcquisitionEditionEffect2(status: "approved" | "rejected", userId: string, now: number, id: string): Effect;
    stageDecideAcquisitionEditionEffect3(status: "approved" | "rejected", userId: string, userId2: string, now: number, calculation_run_id: string | number | null, companyId: string): Effect;
    stageDecideAcquisitionEditionEffect4(value: string, calculation_run_id: string | number | null, action: "approve" | "reject", userId: string, reason: string, value2: string, now: number): Effect;
    readGroupOwnershipWorkspaceOwnershipacquisitionsentities(holdingId: string, companyId: string): Promise<{
        results: (Row)[];
    }>;
    readGroupOwnershipWorkspaceOwnershipacquisitionsentities2(holdingId: string, companyId: string): Promise<{
        results: (Row)[];
    }>;
    readGroupOwnershipWorkspaceOwnershipacquisitionsentities3(holdingId: string): Promise<{
        results: (Row)[];
    }>;
}
export interface FinanceGroupOwnershipPorts<Effect> {
    repository: FinanceGroupOwnershipRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: GroupCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    batch(effects: Effect[]): Promise<unknown>;
    transaction<Result>(run: (ports: FinanceGroupOwnershipPorts<Effect>) => Promise<Result>): Promise<Result>;
    digestBytes(bytes: Uint8Array<ArrayBuffer>): Promise<ArrayBuffer>;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
    requireFinanceTool(...args: Tail<Parameters<typeof requireFinanceTool>>): PortResult<ReturnType<typeof requireFinanceTool>, Effect>;
    requireFinanceMoneyScale(...args: Tail<Parameters<typeof requireFinanceMoneyScale>>): PortResult<ReturnType<typeof requireFinanceMoneyScale>, Effect>;
    sha256(...args: Parameters<typeof sha256>): PortResult<ReturnType<typeof sha256>, Effect>;
}
export function createFinanceGroupOwnershipApplication<Effect>(ports: FinanceGroupOwnershipPorts<Effect>) {
    const repository = ports.repository;
    function actorGuard(actor: WorkContext, permission: string) {
        actor = ports.requirePermission(actor, permission);
        if (actor.scope.kind !== "holding")
            throw new BusinessRuleError(403, "HOLDING_FINANCE_REQUIRED", "Group ownership evidence requires Holding Finance authority.");
        return actor;
    }
    async function get(actor: WorkContext, table: "finance_ownership_interests" | "finance_acquisitions", id: string) {
        const row = await repository.readGetRow(id, actor.holdingId, actor.companyId, table);
        if (!row)
            throw new BusinessRuleError(404, "GROUP_EVIDENCE_NOT_FOUND", "Owned group evidence was not found in this Company.");
        return row;
    }
    async function ownershipSource(actor: WorkContext, input: z.infer<typeof ownershipEditionSchema>) {
        const entities = (await repository.readOwnershipSourceEntities(actor.holdingId, input.investorCompanyId, input.investeeCompanyId)).results;
        if (entities.length !== 2)
            throw new BusinessRuleError(422, "GROUP_ENTITY_REQUIRED", "Register both active Companies before preparing ownership evidence.");
        const parent = input.parentId
            ? await get(actor, "finance_ownership_interests", input.parentId)
            : null;
        if (parent &&
            (parent.status !== "approved" ||
                parent.version !== input.parentVersion ||
                parent.investor_entity_id !==
                    entities.find((e) => e.company_id === input.investorCompanyId)?.id ||
                parent.investee_entity_id !==
                    entities.find((e) => e.company_id === input.investeeCompanyId)?.id ||
                String(parent.effective_from) >= input.effectiveFrom ||
                (parent.effective_to &&
                    String(parent.effective_to) < input.effectiveFrom)))
            throw new BusinessRuleError(409, "OWNERSHIP_PARENT_CHANGED", "Revise the current approved interest with the same entities and a later effective start.");
        const overlap = await repository.readOwnershipSourceOverlap(actor.holdingId, entities.find((e) => e.company_id === input.investorCompanyId)!.id, entities.find((e) => e.company_id === input.investeeCompanyId)!.id, parent?.id ?? "", input.effectiveTo ?? "9999-12-31", input.effectiveFrom);
        if (overlap)
            throw new BusinessRuleError(409, "OWNERSHIP_EFFECTIVE_OVERLAP", "An approved interest already covers this date range. Prepare a dated revision.");
        const evidence = { entities, parent }, hash = await ports.sha256(JSON.stringify({ input, evidence }));
        return { evidence, hash, parent };
    }
    async function prepareOwnershipEdition(actor: WorkContext, raw: z.input<typeof ownershipEditionSchema>, key: string): Promise<Record<string, unknown>> {
        actor = actorGuard(actor, "finance.write");
        const input = ownershipEditionSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        const id = `ownership-${key}`;
        const op = "finance.ownership.prepare";
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts;
            const repository = ports.repository;
            const { ownershipSource } = createFinanceGroupOwnershipApplication(ports).internal;
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            await ports.requireFinanceTool(actor.companyId, "group_reporting");
            const e = await ownershipSource(actor, input), now = ports.clock.now();
            return ports.commands.commit(actor, key, op, input, id, 1, { id, status: "draft", version: 1 }, [
                repository.stagePrepareOwnershipEditionEffect(id, actor.holdingId, actor.companyId, input.parentId ?? null, e.evidence.entities.find((v) => v.company_id === input.investorCompanyId)!.id, e.evidence.entities.find((v) => v.company_id === input.investeeCompanyId)!.id, input.method, input.ownershipBps, input.votingBps, input.controlAssessment, input.effectiveFrom, input.effectiveTo ?? null, actor.userId, now, now, e.hash, JSON.stringify({ input, evidence: e.evidence }), input.reason),
            ]);
        });
    }
    async function decideOwnershipEdition(actor: WorkContext, id: string, raw: z.input<typeof groupEvidenceDecisionSchema>, action: "approve" | "reject", key: string): Promise<Record<string, unknown>> {
        actor = actorGuard(actor, "finance.approve");
        const input = groupEvidenceDecisionSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        const op = `finance.ownership.${action}`;
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts;
            const repository = ports.repository;
            const { get, ownershipSource } = createFinanceGroupOwnershipApplication(ports).internal;
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const r = await get(actor, "finance_ownership_interests", id);
            if (r.status !== "draft" ||
                r.version !== input.expectedVersion ||
                !r.source_json)
                throw new BusinessRuleError(409, "OWNERSHIP_CHANGED", "Review the current owned ownership draft.");
            if (samePerson(actor, r.created_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "Another Finance approver must review ownership evidence.");
            const definition = ownershipEditionSchema.parse(JSON.parse(String(r.source_json)).input), e = action === "approve"
                ? await ownershipSource(actor, definition)
                : null;
            if (e && e.hash !== r.source_hash)
                throw new BusinessRuleError(409, "OWNERSHIP_SOURCE_CHANGED", "Entity or earlier ownership evidence changed. Reject and prepare again.");
            const now = ports.clock.now(), steps = [];
            if (e?.parent) {
                const end = new Date(Date.parse(definition.effectiveFrom) - 86400000)
                    .toISOString()
                    .slice(0, 10);
                steps.push(repository.stageDecideOwnershipEditionEffect(end, now, e.parent.id, e.parent.version));
            }
            steps.push(repository.guard6(key, "ownership-version", id, actor.holdingId, actor.companyId, input.expectedVersion), repository.stageDecideOwnershipEditionEffect2(action === "approve" ? "approved" : "rejected", actor.userId, now, id));
            return ports.commands.commit(actor, key, op, input, id, input.expectedVersion + 1, {
                id,
                status: action === "approve" ? "approved" : "rejected",
                version: input.expectedVersion + 1,
            }, steps);
        });
    }
    async function acquisitionSource(actor: WorkContext, input: z.infer<typeof acquisitionEditionSchema>, currentId = "") {
        await ports.requireFinanceMoneyScale(actor.companyId, input.currency);
        const ownership = await get(actor, "finance_ownership_interests", input.ownershipId);
        if (ownership.version !== input.ownershipVersion ||
            !["approved", "retired"].includes(String(ownership.status)) ||
            !ownership.approved_by ||
            ownership.accounting_method !== "subsidiary" ||
            String(ownership.effective_from) > input.acquisitionDate ||
            (ownership.effective_to &&
                String(ownership.effective_to) < input.acquisitionDate))
            throw new BusinessRuleError(409, "ACQUISITION_OWNERSHIP_REQUIRED", "Select the effective approved subsidiary interest and its exact version.");
        const parent = input.parentId
            ? await get(actor, "finance_acquisitions", input.parentId)
            : null;
        if (parent &&
            (parent.status !== "approved" ||
                parent.version !== input.parentVersion ||
                parent.ownership_id !== input.ownershipId ||
                parent.acquisition_date !== input.acquisitionDate ||
                parent.currency !== input.currency ||
                !parent.source_hash))
            throw new BusinessRuleError(409, "ACQUISITION_PARENT_CHANGED", "Correct the current approved acquisition with the same interest, acquisition date and currency. A new purchase needs its own dated ownership evidence.");
        if (await repository.readAcquisitionSourceEffect(actor.companyId, input.ownershipId, input.acquisitionDate, input.currency, parent?.id ?? "", currentId))
            throw new BusinessRuleError(409, "ACQUISITION_REVISION_REQUIRED", "An acquisition already covers this interest and date. Correct its current approved edition or finish the pending draft first.");
        const net = decimalToUnits(input.identifiableNetAssets) -
            decimalToUnits(input.deferredTax);
        if (net < BigInt(0))
            throw new BusinessRuleError(422, "ACQUISITION_NET_ASSETS_INVALID", "Deferred tax cannot exceed identifiable net assets in this acquisition policy.");
        const nci = input.nciMeasurement === "fair_value"
            ? decimalToUnits(input.nciFairValue)
            : halfAway(net * BigInt(10000 - Number(ownership.ownership_bps)), BigInt(10000));
        const calculation = {
            consideration: input.consideration,
            previousInterest: input.previousInterest,
            fairValueNetAssets: unitsToDecimal(net),
            nciFairValue: unitsToDecimal(nci),
            parentBps: Number(ownership.ownership_bps),
            openingNci: "0",
            subsidiaryProfit: "0",
            subsidiaryOci: "0",
            subsidiaryDividends: "0",
            nciOwnershipChange: "0",
            associateOpening: "0",
            associateBps: 0,
            associateProfit: "0",
            associateOci: "0",
            associateDividends: "0",
            associateImpairment: "0",
        }, result = calculateFinance("consolidation", calculation, 2), evidence = {
            ownership,
            calculation,
            result,
            nci: unitsToDecimal(nci),
            ...(parent ? { parent } : {}),
        }, hash = await ports.sha256(JSON.stringify({ input, evidence }));
        return { evidence, hash, parent };
    }
    async function prepareAcquisitionEdition(actor: WorkContext, raw: z.input<typeof acquisitionEditionSchema>, key: string): Promise<Record<string, unknown>> {
        actor = actorGuard(actor, "finance.write");
        const input = acquisitionEditionSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        const id = `acquisition-${key}`;
        const calcId = `acquisition-calc-${key}`;
        const op = "finance.acquisition.prepare";
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts;
            const repository = ports.repository;
            const { acquisitionSource } = createFinanceGroupOwnershipApplication(ports).internal;
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; calculationRunId: string; result: Record<string,unknown>; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            await ports.requireFinanceTool(actor.companyId, "group_reporting");
            const e = await acquisitionSource(actor, input), now = ports.clock.now(), calcHash = await ports.sha256(JSON.stringify({
                kind: "consolidation",
                input: e.evidence.calculation,
                policyId: null,
                policyVersion: 1,
                currency: input.currency,
                scale: 2,
            }));
            return ports.commands.commit(actor, key, op, input, id, 1, {
                id,
                status: "draft",
                version: 1,
                calculationRunId: calcId,
                result: e.evidence.result,
            }, [
                repository.stagePrepareAcquisitionEditionEffect(calcId, actor.companyId, actor.holdingId, input.currency, calcHash, JSON.stringify(e.evidence.calculation), JSON.stringify(e.evidence.result), JSON.stringify({
                    engine: "source-acquisition-v1",
                    acquisitionId: id,
                    sourceHash: e.hash,
                }), actor.userId, now, now),
                repository.stagePrepareAcquisitionEditionEffect2(ports.identifiers.next(), calcId, actor.userId, input.reason, await ports.sha256(JSON.stringify(e.evidence.result)), now),
                repository.stagePrepareAcquisitionEditionEffect3(id, actor.holdingId, actor.companyId, input.ownershipId, e.evidence.ownership.investee_entity_id, input.acquisitionDate, input.currency, String(decimalToUnits(input.consideration)), String(decimalToUnits(input.previousInterest)), input.nciMeasurement, String(decimalToUnits(e.evidence.nci)), String(decimalToUnits(input.identifiableNetAssets)), String(decimalToUnits(input.deferredTax)), String(decimalToUnits(String(e.evidence.result.goodwill))), String(decimalToUnits(String(e.evidence.result.bargainPurchase))), input.cguReference, calcId, actor.userId, now, now, e.hash, JSON.stringify({ input, evidence: e.evidence }), input.reason, input.parentId ?? null, input.parentVersion ?? null),
            ]);
        });
    }
    async function decideAcquisitionEdition(actor: WorkContext, id: string, raw: z.input<typeof groupEvidenceDecisionSchema>, action: "approve" | "reject", key: string): Promise<Record<string, unknown>> {
        actor = actorGuard(actor, "finance.approve");
        const input = groupEvidenceDecisionSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        const op = `finance.acquisition.${action}`;
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts;
            const repository = ports.repository;
            const { get, acquisitionSource } = createFinanceGroupOwnershipApplication(ports).internal;
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const r = await get(actor, "finance_acquisitions", id);
            if (r.status !== "draft" ||
                r.version !== input.expectedVersion ||
                !r.source_json)
                throw new BusinessRuleError(409, "ACQUISITION_CHANGED", "Review the current acquisition draft.");
            if (samePerson(actor, r.created_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "Another Finance approver must review acquisition evidence.");
            const definition = acquisitionEditionSchema.parse(JSON.parse(String(r.source_json)).input), e = action === "approve"
                ? await acquisitionSource(actor, definition, id)
                : null;
            if (e && e.hash !== r.source_hash)
                throw new BusinessRuleError(409, "ACQUISITION_SOURCE_CHANGED", "Ownership or calculation evidence changed. Reject and prepare again.");
            if (action === "approve") {
                const calc = await repository.readDecideAcquisitionEditionCalc(r.calculation_run_id, actor.companyId, actor.holdingId);
                const expected = JSON.parse(String(r.source_json)).evidence;
                if (!calc ||
                    calc.input_json !== JSON.stringify(expected.calculation) ||
                    calc.result_json !== JSON.stringify(expected.result) ||
                    JSON.parse(String(calc.explanation_json)).acquisitionId !== id)
                    throw new BusinessRuleError(409, "ACQUISITION_CALCULATION_CHANGED", "The owned acquisition calculation changed before its independent review.");
            }
            const now = ports.clock.now(), status = action === "approve" ? "approved" : "rejected";
            return ports.commands.commit(actor, key, op, input, id, input.expectedVersion + 1, { id, status, version: input.expectedVersion + 1 }, [
                ...(action === "approve" && e?.parent
                    ? [
                        repository.guard13(key, "acquisition-parent", e.parent.id, actor.companyId, actor.holdingId, Number(definition.parentVersion)),
                        repository.stageDecideAcquisitionEditionEffect(now, e.parent.id),
                    ]
                    : []),
                repository.guard15(key, "acquisition-version", id, actor.companyId, actor.holdingId, input.expectedVersion),
                repository.stageDecideAcquisitionEditionEffect2(status, actor.userId, now, id),
                repository.stageDecideAcquisitionEditionEffect3(status, actor.userId, actor.userId, now, r.calculation_run_id, actor.companyId),
                repository.stageDecideAcquisitionEditionEffect4(ports.identifiers.next(), r.calculation_run_id, action, actor.userId, input.reason, await ports.sha256(JSON.stringify(JSON.parse(String(r.source_json)).evidence.result)), now),
            ]);
        });
    }
    async function groupOwnershipWorkspace(actor: WorkContext) {
        actor = actorGuard(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const [ownership, acquisitions, entities] = await Promise.all([
            repository.readGroupOwnershipWorkspaceOwnershipacquisitionsentities(actor.holdingId, actor.companyId),
            repository.readGroupOwnershipWorkspaceOwnershipacquisitionsentities2(actor.holdingId, actor.companyId),
            repository.readGroupOwnershipWorkspaceOwnershipacquisitionsentities3(actor.holdingId),
        ]);
        return {
            ownership: ownership.results,
            acquisitions: acquisitions.results,
            entities: entities.results,
        };
    }
    return { prepareOwnershipEdition, decideOwnershipEdition, prepareAcquisitionEdition, decideAcquisitionEdition, groupOwnershipWorkspace, internal: { actorGuard, get, ownershipSource, prepareOwnershipEdition, decideOwnershipEdition, acquisitionSource, prepareAcquisitionEdition, decideAcquisitionEdition, groupOwnershipWorkspace } };
}


