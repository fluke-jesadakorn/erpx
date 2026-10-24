import { translationRate, translateMinor } from '@erpx/server-finance/domain/group-consolidation-math';
import { z } from "zod";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {decimalToMinor} from '@erpx/server-finance/public/finance';
import type {minorToDecimal} from '@erpx/server-finance/public/finance';
import { type EntityPeriod, type ConsolidationRun, type ConsolidationElimination, type ConsolidationAccount, consolidationRunSchema, consolidationDecisionSchema, consolidationEliminationSchema, consolidationEliminationDecisionSchema, type EliminationAction, eliminationTransition, transition } from '@erpx/server-finance/domain/finance-consolidation';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { GroupCommands } from '@erpx/server-finance/domain/group-command-ports';
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: infer Args) => infer Result ? (...args: Args) => PortResult<Result, Effect> : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface FinanceConsolidationRepository<Effect> {
    readConsolidationRunWorkspaceRunseliminationsaccounts(holdingId: string): Promise<{
        results: (ConsolidationRun)[];
    }>;
    readConsolidationRunWorkspaceRunseliminationsaccounts2(holdingId: string): Promise<{
        results: (ConsolidationElimination)[];
    }>;
    readConsolidationRunWorkspaceRunseliminationsaccounts3(holdingId: string): Promise<{
        results: (ConsolidationAccount)[];
    }>;
    readEntityPeriodsEntities(holdingId: string): Promise<{
        results: ({
            company_id: string;
            reporting_name: string;
            functional_currency: string;
        })[];
    }>;
    readEntityPeriodsPeriod(company_id: string, asOfDate: string): Promise<{
        id: string;
        name: string;
        end_date: string;
        close_version: number;
    } | null>;
    readSnapshotAccounts(companyId: string, periodId: string): Promise<{
        results: (Record<string, string | number | null>)[];
    }>;
    readSnapshotIntercompany(holdingId: string): Promise<{
        results: (Record<string, string | number | null>)[];
    }>;
    readConsolidationAccountsRows(holdingId: string, unique: string[]): Promise<{
        results: (ConsolidationAccount)[];
    }>;
    readConsolidationRunRun(runId: string, holdingId: string): Promise<ConsolidationRun | null>;
    readCreateConsolidationEliminationTransaction(sourceTransactionId: string, holdingId: string): Promise<{
        id: string;
        status: string;
        currency: string;
        amount_minor: number;
        source_company_id: string;
        target_company_id: string;
    } | null>;
    guard11(key: string, value: "elimination-unique-source", runId: string, sourceTransactionId: string): Effect;
    stageCreateConsolidationEliminationEffect(id: string, runId: string, description: string, debitAccountId: string, creditAccountId: string, amount: number, sourceTransactionId: string, userId: string, now: number, now2: number, value: string): Effect;
    stageCreateConsolidationEliminationEffect2(value: string, id: string, runId: string, holdingId: string, value2: "proposed", value3: null, value4: "proposed", userId: string, value5: 1, value6: "Elimination account pair prepared from a settled intercompany source.", value7: string, now: number): Effect;
    readDecideConsolidationEliminationRow(eliminationId: string, holdingId: string): Promise<ConsolidationElimination & {
        run_status: string;
        as_of_date: string;
        presentation_currency: string;
        source_hash: string;
        source_snapshot_json: string;
    } | null>;
    guard15(key: string, value: "elimination-version", eliminationId: string, expectedVersion: number, from: "proposed" | "approved"): Effect;
    stageDecideConsolidationEliminationEffect(userId: string, now: number, now2: number, eliminationId: string, expectedVersion: number): Effect;
    readDecideConsolidationEliminationLineCount(journalId: string): Promise<{
        count: number;
    } | null>;
    guard18(key: string, value: "elimination-journal-not-reversed", journalId: string): Effect;
    guard19(key: string, value: "elimination-lines-unique", value2: string): Effect;
    stageDecideConsolidationEliminationEffect2(journalId: string, run_id: string, holdingId: string, journalNumber: string, as_of_date: string, presentation_currency: string, value: "Controlled Group Finance elimination journal", source_hash: string, created_by: string | number | null, userId: string, now: number, now2: number, now3: number): Effect;
    stageDecideConsolidationEliminationEffect3(value: string, journalId: string, debit_account_id: string, description: string, amount_minor: number, ordinal: number): Effect;
    stageDecideConsolidationEliminationEffect4(value: string, journalId: string, credit_account_id: string, description: string, amount_minor: number, value2: number): Effect;
    stageDecideConsolidationEliminationEffect5(userId: string, now: number, now2: number, journalId: string, run_id: string): Effect;
    stageDecideConsolidationEliminationEffect6(journalId: string, userId: string, now: number, now2: number, eliminationId: string, expectedVersion: number): Effect;
    stageDecideConsolidationEliminationEffect7(value: string, eliminationId: string, run_id: string, holdingId: string, event: "approved" | "posted", status: "proposed" | "approved", to: "approved" | "posted", userId: string, version: number, reason: string, value2: string, now: number): Effect;
    readCreateConsolidationRunDuplicate(holdingId: string, asOfDate: string, presentationCurrency: string): Promise<{
        id: string;
    } | null>;
    guard27(key: string, value: "consolidation-unique", holdingId: string, asOfDate: string, presentationCurrency: string): Effect;
    stageCreateConsolidationRunEffect(id: string, holdingId: string, presentationCurrency: string, asOfDate: string, value: string, sourceHash: string, userId: string, now: number, now2: number): Effect;
    stageCreateConsolidationRunEffect2(value: string, id: string, holdingId: string, value2: "prepared", value3: null, value4: "draft", userId: string, value5: 1, value6: "Consolidation snapshot prepared from closed company periods.", value7: string, now: number): Effect;
    readDecideConsolidationRunRun(runId: string, holdingId: string): Promise<ConsolidationRun | null>;
    readDecideConsolidationRunPending(runId: string): Promise<Record<string, unknown> | null>;
    readDecideConsolidationRunPending2(runId: string): Promise<Record<string, unknown> | null>;
    stageDecideConsolidationRunUpdate(action: keyof typeof transition, userId: string, now: number, runId: string, holdingId: string, expectedVersion: number): Effect;
    stageDecideConsolidationRunEvent(value: string, runId: string, holdingId: string, event: "approved" | "reviewed" | "published", status: "approved" | "draft" | "review", to: "approved" | "review" | "published", userId: string, version: number, reason: string, value2: string, now: number): Effect;
    guard35(key: string, value: "consolidation-version", runId: string, holdingId: string, from: "approved" | "draft" | "review", expectedVersion: number): Effect;
    readConsolidationDetailRow(id: string, holdingId: string): Promise<ConsolidationRun | null>;
    readConsolidationDetailEffect(id: string): Promise<{
        results: (ConsolidationElimination)[];
    }>;
    readConsolidationEliminationDetailElimination(id: string, holdingId: string): Promise<ConsolidationElimination | null>;
}
export interface FinanceConsolidationPorts<Effect> {
    repository: FinanceConsolidationRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: GroupCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    batch(effects: Effect[]): Promise<unknown>;
    transaction<Result>(run: (ports: FinanceConsolidationPorts<Effect>) => Promise<Result>): Promise<Result>;
    digestBytes(bytes: Uint8Array<ArrayBuffer>): Promise<ArrayBuffer>;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
    decimalToMinor(...args: Parameters<typeof decimalToMinor>): PortResult<ReturnType<typeof decimalToMinor>, Effect>;
    minorToDecimal(...args: Parameters<typeof minorToDecimal>): PortResult<ReturnType<typeof minorToDecimal>, Effect>;
}
export function createFinanceConsolidationApplication<Effect>(ports: FinanceConsolidationPorts<Effect>) {
    const repository = ports.repository;
    async function consolidationRunWorkspace(actor: WorkContext) {
        actor = holdingActor(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const [runs, eliminations, accounts] = await Promise.all([
            repository.readConsolidationRunWorkspaceRunseliminationsaccounts(actor.holdingId),
            repository.readConsolidationRunWorkspaceRunseliminationsaccounts2(actor.holdingId),
            repository.readConsolidationRunWorkspaceRunseliminationsaccounts3(actor.holdingId),
        ]);
        const byRun = new Map<string, ConsolidationElimination[]>();
        for (const elimination of eliminations.results)
            byRun.set(elimination.run_id, [...(byRun.get(elimination.run_id) ?? []), elimination]);
        return {
            runs: runs.results.map(run => ({ ...run, eliminations: byRun.get(run.id) ?? [] })),
            accounts: accounts.results,
            canWrite: hasPermission(actor, "finance.write"),
            canApprove: hasPermission(actor, "finance.approve"),
        };
    }
    function holdingActor(actor: WorkContext, permission: string) {
        actor = ports.requirePermission(actor, permission);
        if (actor.scope.kind !== "holding")
            throw new BusinessRuleError(403, "HOLDING_FINANCE_REQUIRED", "Group consolidation requires a Holding-scoped Finance assignment.");
        return actor;
    }
    async function digest(value: unknown) {
        return Array.from(new Uint8Array(await ports.digestBytes(new TextEncoder().encode(JSON.stringify(value))))).map(byte => byte.toString(16).padStart(2, "0")).join("");
    }
    async function entityPeriods(holdingId: string, asOfDate: string) {
        const entities = await repository.readEntityPeriodsEntities(holdingId);
        if (entities.results.length < 2)
            throw new BusinessRuleError(409, "CONSOLIDATION_ENTITIES_REQUIRED", "Register at least two active Group Finance entities before preparing consolidation.");
        const result: EntityPeriod[] = [];
        for (const entity of entities.results) {
            const period = await repository.readEntityPeriodsPeriod(entity.company_id, asOfDate);
            if (!period)
                throw new BusinessRuleError(409, "CONSOLIDATION_PERIODS_REQUIRED", `${entity.reporting_name} must have a closed accounting period on or before ${asOfDate}.`);
            result.push({ companyId: entity.company_id, reportingName: entity.reporting_name, functionalCurrency: entity.functional_currency, periodId: period.id, periodName: period.name, endDate: period.end_date, closeVersion: Number(period.close_version) });
        }
        return result;
    }
    async function snapshot(holdingId: string, asOfDate: string, presentationCurrency: string, rawRates: Array<{
        currency: string;
        rate: string;
    }>) {
        const rates = new Map<string, number>();
        for (const row of rawRates) {
            if (rates.has(row.currency))
                throw new BusinessRuleError(422, "DUPLICATE_CONSOLIDATION_RATE", "Provide one translation rate per currency.");
            rates.set(row.currency, ports.decimalToMinor(row.rate, 6));
        }
        const entityPeriodsResult = await entityPeriods(holdingId, asOfDate);
        const companies = [] as Array<Record<string, unknown>>;
        for (const entity of entityPeriodsResult) {
            const rateMicros = translationRate(entity.functionalCurrency, presentationCurrency, rates);
            const accounts = await repository.readSnapshotAccounts(entity.companyId, entity.periodId);
            companies.push({ ...entity, translationRateMicros: rateMicros, accounts: accounts.results.map(account => ({ ...account, translatedBalanceMinor: translateMinor(Number(account.balance_minor ?? 0), rateMicros) })) });
        }
        const intercompany = await repository.readSnapshotIntercompany(holdingId);
        const source = { asOfDate, presentationCurrency, translationRates: Object.fromEntries(rates), companies, intercompany: intercompany.results };
        return { source, sourceHash: await digest(source) };
    }
    async function consolidationAccounts(holdingId: string, accountIds: string[], currency: string) {
        const unique = [...new Set(accountIds)];
        const rows = await repository.readConsolidationAccountsRows(holdingId, unique);
        if (rows.results.length !== unique.length)
            throw new BusinessRuleError(422, "CONSOLIDATION_ACCOUNT_INVALID", "Choose two active accounts from the Group Finance chart of accounts.");
        if (unique.length !== 2 || unique[0] === unique[1])
            throw new BusinessRuleError(422, "CONSOLIDATION_ACCOUNT_PAIR_REQUIRED", "Elimination entries require different debit and credit accounts.");
        if (rows.results.some(row => row.currency !== currency || row.base_currency !== currency))
            throw new BusinessRuleError(422, "CONSOLIDATION_ACCOUNT_CURRENCY", `Elimination accounts must use the ${currency} presentation currency.`);
        if (new Set(rows.results.map(row => row.chart_id)).size !== 1)
            throw new BusinessRuleError(422, "CONSOLIDATION_ACCOUNT_CHART", "Elimination accounts must belong to one Group Finance chart.");
        return new Map(rows.results.map(row => [row.id, row]));
    }
    async function consolidationRun(actor: WorkContext, runId: string) {
        const run = await repository.readConsolidationRunRun(runId, actor.holdingId);
        if (!run)
            throw new BusinessRuleError(404, "CONSOLIDATION_RUN_NOT_FOUND", "Consolidation run not found in this Holding.");
        return run;
    }
    async function createConsolidationElimination(actor: WorkContext, runId: string, input: z.infer<typeof consolidationEliminationSchema>, key: string) {
        actor = holdingActor(actor, "finance.write");
        await ports.prepare(actor.holdingId);
        const id = `elimination-${key}`;
        const previous = await ports.commands.replay<{ id: string; runId: string; status: string; version: number; amountMinor: number; }>(actor, key, "finance.consolidation.elimination.propose", id, { runId, ...input });
        if (previous)
            return previous;
        const run = await consolidationRun(actor, runId);
        if (run.status !== "draft")
            throw new BusinessRuleError(409, "CONSOLIDATION_ELIMINATION_LOCKED", "Add elimination accounts before the consolidation run enters review.");
        const transaction = await repository.readCreateConsolidationEliminationTransaction(input.sourceTransactionId, actor.holdingId);
        if (!transaction)
            throw new BusinessRuleError(422, "CONSOLIDATION_SOURCE_REQUIRED", "Choose a settled intercompany transaction as the elimination source.");
        const amount = ports.decimalToMinor(input.amount);
        if (amount <= 0)
            throw new BusinessRuleError(422, "CONSOLIDATION_AMOUNT_INVALID", "The elimination amount must be greater than zero.");
        const accounts = await consolidationAccounts(actor.holdingId, [input.debitAccountId, input.creditAccountId], run.presentation_currency);
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "finance.consolidation.elimination.propose", { runId, ...input }, id, 1, { id, runId, status: "proposed", version: 1, amountMinor: amount }, [
            repository.guard11(key, "elimination-unique-source", runId, input.sourceTransactionId),
            repository.stageCreateConsolidationEliminationEffect(id, runId, input.description, input.debitAccountId, input.creditAccountId, amount, input.sourceTransactionId, actor.userId, now, now, JSON.stringify({ sourceTransactionId: transaction.id, sourceStatus: transaction.status, sourceAmountMinor: transaction.amount_minor, debitAccount: accounts.get(input.debitAccountId)?.code, creditAccount: accounts.get(input.creditAccountId)?.code, sourceHash: run.source_hash })),
            repository.stageCreateConsolidationEliminationEffect2(`elimination-event-${key}`, id, runId, actor.holdingId, "proposed", null, "proposed", actor.userId, 1, "Elimination account pair prepared from a settled intercompany source.", JSON.stringify({ sourceTransactionId: input.sourceTransactionId, sourceHash: run.source_hash }), now),
        ]);
    }
    async function decideConsolidationElimination(actor: WorkContext, eliminationId: string, action: EliminationAction, input: z.infer<typeof consolidationEliminationDecisionSchema>, key: string) {
        const rule = eliminationTransition[action];
        actor = holdingActor(actor, rule.permission);
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, rule.operation, eliminationId, input);
        if (previous)
            return previous;
        const row = await repository.readDecideConsolidationEliminationRow(eliminationId, actor.holdingId);
        if (!row)
            throw new BusinessRuleError(404, "CONSOLIDATION_ELIMINATION_NOT_FOUND", "Consolidation elimination not found in this Holding.");
        if (row.status !== rule.from || Number(row.version) !== input.expectedVersion)
            throw new BusinessRuleError(409, "CONSOLIDATION_ELIMINATION_CHANGED", `The elimination is no longer ${rule.from}; reload before deciding.`);
        if (action === "approve" && !["review", "approved"].includes(row.run_status))
            throw new BusinessRuleError(409, "CONSOLIDATION_RUN_NOT_IN_REVIEW", "Review the consolidation snapshot before approving its elimination accounts.");
        if (action === "post" && row.run_status !== "approved")
            throw new BusinessRuleError(409, "CONSOLIDATION_RUN_NOT_APPROVED", "Approve the consolidation snapshot before posting elimination accounts.");
        if (action === "approve" && row.created_by === actor.userId)
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different authorized Finance approver must approve the elimination.");
        if (action === "post" && row.approved_by === actor.userId)
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different authorized Finance approver must post the approved elimination.");
        const storedSource = JSON.parse(String(row.source_snapshot_json)) as {
            translationRates?: Record<string, string | number>;
        };
        const current = await snapshot(actor.holdingId, row.as_of_date, row.presentation_currency, Object.entries(storedSource.translationRates ?? {}).map(([currency, rate]) => ({ currency, rate: ports.minorToDecimal(Number(rate), 6) })));
        if (current.sourceHash !== row.source_hash)
            throw new BusinessRuleError(409, "CONSOLIDATION_SNAPSHOT_STALE", "Closed periods or intercompany evidence changed. Prepare a fresh consolidation snapshot.");
        const accounts = await consolidationAccounts(actor.holdingId, [row.debit_account_id, row.credit_account_id], row.presentation_currency);
        const version = input.expectedVersion + 1, now = ports.clock.now();
        const steps: Effect[] = [repository.guard15(key, "elimination-version", eliminationId, input.expectedVersion, rule.from)];
        let result: Record<string, unknown> = { id: eliminationId, status: rule.to, version };
        if (action === "approve") {
            steps.push(repository.stageDecideConsolidationEliminationEffect(actor.userId, now, now, eliminationId, input.expectedVersion));
        }
        else {
            const journalId = `consolidation-journal-${row.run_id}`;
            const journalNumber = `CONSOLIDATION-${row.run_id}`;
            const lineCount = await repository.readDecideConsolidationEliminationLineCount(journalId);
            const ordinal = Number(lineCount?.count ?? 0);
            steps.push(repository.guard18(key, "elimination-journal-not-reversed", journalId), repository.guard19(key, "elimination-lines-unique", `${eliminationId}-debit`), repository.stageDecideConsolidationEliminationEffect2(journalId, row.run_id, actor.holdingId, journalNumber, row.as_of_date, row.presentation_currency, "Controlled Group Finance elimination journal", row.source_hash, row.created_by, actor.userId, now, now, now), repository.stageDecideConsolidationEliminationEffect3(`${eliminationId}-debit`, journalId, row.debit_account_id, row.description, row.amount_minor, ordinal), repository.stageDecideConsolidationEliminationEffect4(`${eliminationId}-credit`, journalId, row.credit_account_id, row.description, row.amount_minor, ordinal + 1), repository.stageDecideConsolidationEliminationEffect5(actor.userId, now, now, journalId, row.run_id), repository.stageDecideConsolidationEliminationEffect6(journalId, actor.userId, now, now, eliminationId, input.expectedVersion));
            result = { ...result, journalId };
        }
        steps.push(repository.stageDecideConsolidationEliminationEffect7(`elimination-event-${key}`, eliminationId, row.run_id, actor.holdingId, rule.event, row.status, rule.to, actor.userId, version, input.reason, JSON.stringify({ sourceHash: row.source_hash, debitAccount: accounts.get(row.debit_account_id)?.code, creditAccount: accounts.get(row.credit_account_id)?.code, amountMinor: row.amount_minor }), now));
        return ports.commands.commit(actor, key, rule.operation, input, eliminationId, version, result, steps);
    }
    async function createConsolidationRun(actor: WorkContext, input: z.infer<typeof consolidationRunSchema>, key: string) {
        actor = holdingActor(actor, "finance.write");
        await ports.prepare(actor.holdingId);
        const id = `consolidation-${key}`;
        const previous = await ports.commands.replay<{ id: string; status: string; version: number; sourceHash: string; companyCount: number; }>(actor, key, "finance.consolidation.prepare", id, input);
        if (previous)
            return previous;
        const duplicate = await repository.readCreateConsolidationRunDuplicate(actor.holdingId, input.asOfDate, input.presentationCurrency);
        if (duplicate)
            throw new BusinessRuleError(409, "CONSOLIDATION_RUN_EXISTS", "A consolidation run already exists for this date and presentation currency.");
        const { source, sourceHash } = await snapshot(actor.holdingId, input.asOfDate, input.presentationCurrency, input.translationRates);
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "finance.consolidation.prepare", input, id, 1, { id, status: "draft", version: 1, sourceHash, companyCount: source.companies.length }, [
            repository.guard27(key, "consolidation-unique", actor.holdingId, input.asOfDate, input.presentationCurrency),
            repository.stageCreateConsolidationRunEffect(id, actor.holdingId, input.presentationCurrency, input.asOfDate, JSON.stringify(source), sourceHash, actor.userId, now, now),
            repository.stageCreateConsolidationRunEffect2(`consolidation-event-${key}`, id, actor.holdingId, "prepared", null, "draft", actor.userId, 1, "Consolidation snapshot prepared from closed company periods.", JSON.stringify({ sourceHash, companyCount: source.companies.length }), now),
        ]);
    }
    async function decideConsolidationRun(actor: WorkContext, runId: string, action: keyof typeof transition, input: z.infer<typeof consolidationDecisionSchema>, key: string) {
        const rule = transition[action];
        actor = holdingActor(actor, rule.permission);
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay<{ id: string; status: "approved" | "review" | "published"; version: number; }>(actor, key, rule.operation, runId, input);
        if (previous)
            return previous;
        const run = await repository.readDecideConsolidationRunRun(runId, actor.holdingId);
        if (!run)
            throw new BusinessRuleError(404, "CONSOLIDATION_RUN_NOT_FOUND", "Consolidation run not found in this Holding.");
        if (run.status !== rule.from || Number(run.version) !== input.expectedVersion)
            throw new BusinessRuleError(409, "CONSOLIDATION_CHANGED", `The consolidation run is no longer ${rule.from}; reload before deciding.`);
        if (run.created_by === actor.userId || (action === "approve" && run.reviewed_by === actor.userId) || (action === "publish" && run.approved_by === actor.userId))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different authorized Finance colleague must perform this handoff.");
        if (action === "approve") {
            const pending = await repository.readDecideConsolidationRunPending(runId);
            if (pending)
                throw new BusinessRuleError(409, "CONSOLIDATION_ELIMINATIONS_UNREVIEWED", "Approve every proposed elimination account before approving the consolidation run.");
        }
        if (action === "publish") {
            const pending = await repository.readDecideConsolidationRunPending2(runId);
            if (pending)
                throw new BusinessRuleError(409, "CONSOLIDATION_ELIMINATIONS_UNPOSTED", "Post every approved elimination journal before publishing the consolidation run.");
        }
        const storedSource = JSON.parse(String(run.source_snapshot_json)) as {
            translationRates?: Record<string, string | number>;
        };
        const current = await snapshot(actor.holdingId, run.as_of_date, run.presentation_currency, Object.entries(storedSource.translationRates ?? {}).map(([currency, rate]) => ({ currency, rate: ports.minorToDecimal(Number(rate), 6) })));
        if (current.sourceHash !== run.source_hash)
            throw new BusinessRuleError(409, "CONSOLIDATION_SNAPSHOT_STALE", "Closed periods or intercompany evidence changed. Prepare a fresh consolidation snapshot.");
        const version = input.expectedVersion + 1, now = ports.clock.now();
        const update = repository.stageDecideConsolidationRunUpdate(action, actor.userId, now, runId, actor.holdingId, input.expectedVersion);
        const event = repository.stageDecideConsolidationRunEvent(`consolidation-event-${key}`, runId, actor.holdingId, rule.event, run.status, rule.to, actor.userId, version, input.reason, JSON.stringify({ sourceHash: run.source_hash, previousVersion: input.expectedVersion }), now);
        const steps: Effect[] = [
            repository.guard35(key, "consolidation-version", runId, actor.holdingId, rule.from, input.expectedVersion),
            update,
            event,
        ];
        return ports.commands.commit(actor, key, rule.operation, input, runId, version, { id: runId, status: rule.to, version }, steps);
    }
    async function consolidationDetail(actor: WorkContext, id: string) {
        actor = holdingActor(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const row = await repository.readConsolidationDetailRow(id, actor.holdingId);
        if (!row)
            throw new BusinessRuleError(404, "CONSOLIDATION_NOT_FOUND", "Consolidation run not found.");
        const data = await consolidationRunWorkspace(actor);
        return { row, data: { ...data, runs: [row], eliminations: (await repository.readConsolidationDetailEffect(id)).results } };
    }
    async function consolidationEliminationDetail(actor: WorkContext, id: string) {
        actor = holdingActor(actor, 'finance.read');
        await ports.prepare(actor.holdingId);
        const elimination = await repository.readConsolidationEliminationDetailElimination(id, actor.holdingId);
        if (!elimination)
            throw new BusinessRuleError(404, 'ELIMINATION_NOT_FOUND', 'Elimination not found.');
        const detail = await consolidationDetail(actor, String(elimination.run_id));
        return { row: elimination, data: { ...detail.data, eliminations: [elimination] } };
    }
    return { consolidationRunWorkspace, createConsolidationElimination, decideConsolidationElimination, createConsolidationRun, decideConsolidationRun, consolidationDetail, consolidationEliminationDetail, internal: { consolidationRunWorkspace, holdingActor, digest, entityPeriods, translationRate, translateMinor, snapshot, consolidationAccounts, consolidationRun, createConsolidationElimination, decideConsolidationElimination, createConsolidationRun, decideConsolidationRun, consolidationDetail, consolidationEliminationDetail } };
}


