import { z } from 'zod';
import { depreciationAmount, monthlyDate } from '@erpx/server-finance/domain/finance-instrument-policy';
import { assetPolicySchema, assetPolicyDecisionSchema, assetPolicyReversalSchema, type Row, type Book, type Result } from '@erpx/server-finance/domain/asset-policies';
import { assetPolicyRunSchema } from '@erpx/server-finance/domain/due-inputs';
import { decimalToUnits, unitsToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { JournalInput, JournalLineInput } from '@erpx/server-finance/domain/finance';
import type { assetCarryingEvidence, AssetCarryBasis } from '@erpx/server-finance/public/finance-asset-carrying';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface AssetPolicyRepository<Effect> {
    asset(id: string, companyId: string): Promise<Row | null>;
    pendingDisposal(id: string): Promise<Record<string, unknown> | null>;
    bookRules(companyId: string, chart_id: string | number | null, date: string): Promise<{
        results: Row[];
    }>;
    bookMappings(id: string | number | null, account_id: string | number | null, depreciation_account_id: string | number | null, depreciation_expense_account_id: string | number | null): Promise<{
        results: Row[];
    }>;
    bookAccounts(companyId: string, accountIds: string[]): Promise<{
        results: Row[];
    }>;
    edition(id: string, companyId: string): Promise<Row | null>;
    legacyCount(id: string | number | null): Promise<number | null>;
    postedRuns(id: string | number | null): Promise<{
        results: Row[];
    }>;
    policyLegacyCount(id: string | number | null): Promise<number | null>;
    pendingRun(id: string | number | null): Promise<Record<string, unknown> | null>;
    preparePolicy(id: string, companyId: string, assetId: string, value: string | number | null, value2: string, hash: string, userId: string, reason: string, now: number, now2: number, value3: string): Effect;
    supersedePolicy(now: number, id: string | number | null): Effect;
    guardPolicyVersion(key: string, value: "asset-policy-version", id: string, companyId: string, expectedVersion: number): Effect;
    decidePolicy(value: "active" | "rejected", userId: string, now: number, id: string): Effect;
    openPeriod(periodId: string, companyId: string, entryDate: string, entryDate2: string): Promise<Record<string, unknown> | null>;
    runLegacyCount(id: string | number | null): Promise<number | null>;
    prepareRun(runId: string, companyId: string, id: string | number | null, id2: string, ordinal: number, entryDate: string, value: string, value2: string, hash: string, userId: string, reason: string, now: number, now2: number): Effect;
    run(id: string, companyId: string): Promise<Row | null>;
    updateAssetCarrying(value: string, now: number, id: string | number | null): Effect;
    updatePolicyVersion(now: number, id: string | number | null): Effect;
    assetEvent(value: string, id: string | number | null, companyId: string, value2: number, key: string, value3: string, userId: string, now: number): Effect;
    guardRunVersion(key: string, value: "asset-policy-run-version", id: string, companyId: string, expectedVersion: number): Effect;
    decideRun(value: "posted" | "rejected", userId: string, value2: string, now: number, id: string): Effect;
    laterRuns(id: string | number | null, ordinal: string | number | null): Promise<Record<string, unknown> | null>;
    pendingPolicy(id: string | number | null): Promise<Record<string, unknown> | null>;
    reverseAssetCarrying(value: string, now: number, id: string | number | null): Effect;
    reversePolicyVersion(now: number, id: string | number | null): Effect;
    reverseRun(value: string, entryDate: string, now: number, id: string): Effect;
    workspacePolicies(companyId: string): Promise<{
        results: Row[];
    }>;
    workspaceRuns(companyId: string): Promise<{
        results: Row[];
    }>;
    workspaceAssets(companyId: string): Promise<{
        results: Row[];
    }>;
    workspaceBooks(companyId: string): Promise<{
        results: Row[];
    }>;
    workspacePeriods(companyId: string): Promise<{
        results: Row[];
    }>;
    activeDisposalPolicy(assetId: string, companyId: string): Promise<Row | null>;
    disposalAsset(assetId: string, companyId: string): Promise<Row | null>;
    disposalPendingRun(assetId: string): Promise<Record<string, unknown> | null>;
    disposalPendingPolicy(assetId: string): Promise<Record<string, unknown> | null>;
    disposalBookMappings(ruleId: string | null, gainAccountId: string, lossAccountId: string): Promise<{
        results: Row[];
    }>;
}
export interface AssetPolicyPorts<Effect> {
    repository: AssetPolicyRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OwnerCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    hash(value: string): Promise<string>;
    requirePermission(actor: WorkContext, permission: 'finance.read' | 'finance.write' | 'finance.approve'): WorkContext;
    requireTool(companyId: string, tool: 'asset_policies'): Promise<unknown>;
    transaction<Value>(work: (ports: AssetPolicyPorts<Effect>) => Promise<Value>): Promise<Value>;
    ownedTransaction<Value>(available: boolean, work: (ports: AssetPolicyPorts<Effect>) => Promise<Value>): Promise<Value>;
    carrying(asset: Row, books: Book[]): Promise<Awaited<ReturnType<typeof assetCarryingEvidence>>>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string, status: 'draft' | 'posted'): Promise<{
        result: {
            id: string;
        };
        steps: Effect[];
    }>;
    prepareReversal(actor: WorkContext, journalId: string, sourceType: 'book_adjustment' | 'asset_policy_depreciation', sourceId: string, input: {
        periodId: string;
        entryDate: string;
        reason: string;
    }, key: string): Promise<{
        id: string;
        steps: Effect[];
    }>;
}
export function createAssetPolicyApplication<Effect>(ports: AssetPolicyPorts<Effect>) {
    const repository = ports.repository;
    async function asset(actor: WorkContext, id: string) {
        const row = await repository.asset(id, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "ASSET_NOT_FOUND", "Asset not found.");
        if (row.status !== "in_service" ||
            !row.activation_journal_id ||
            !row.depreciation_account_id ||
            !row.depreciation_expense_account_id)
            throw new BusinessRuleError(409, "ASSET_NOT_RECOGNIZED", "Recognize the asset with complete depreciation mappings first.");
        if (await repository.pendingDisposal(id))
            throw new BusinessRuleError(409, "ASSET_DISPOSAL_PENDING", "Decide the asset disposal before preparing more depreciation.");
        return row;
    }
    async function bookEvidence(actor: WorkContext, a: Row, definition: z.infer<typeof assetPolicySchema>, date: string) {
        const rules = (await repository.bookRules(actor.companyId, a.chart_id, date)).results;
        const ids = [
            String(a.chart_id),
            ...rules.map((r) => String(r.secondary_chart_id)),
        ];
        if (definition.books.length !== ids.length ||
            new Set(definition.books.map((b) => b.chartId)).size !== ids.length ||
            ids.some((id) => !definition.books.some((b) => b.chartId === id)))
            throw new BusinessRuleError(409, "ASSET_ALL_BOOK_POLICIES_REQUIRED", "Review a depreciation policy for the primary book and every enabled secondary book.");
        const books: Book[] = [
            {
                chartId: String(a.chart_id),
                costAccount: String(a.account_id),
                accumulatedAccount: String(a.depreciation_account_id),
                expenseAccount: String(a.depreciation_expense_account_id),
                ruleId: null,
            },
        ];
        for (const r of rules) {
            const mappings = (await repository.bookMappings(r.id, a.account_id, a.depreciation_account_id, a.depreciation_expense_account_id)).results, byId = new Map(mappings.map((m) => [
                m.primary_account_id,
                String(m.secondary_account_id),
            ]));
            if (mappings.length !== 3)
                throw new BusinessRuleError(409, "ASSET_BOOK_MAPPING_REQUIRED", "Complete every book's asset mappings.");
            books.push({
                chartId: String(r.secondary_chart_id),
                costAccount: byId.get(a.account_id!)!,
                accumulatedAccount: byId.get(a.depreciation_account_id!)!,
                expenseAccount: byId.get(a.depreciation_expense_account_id!)!,
                ruleId: String(r.id),
            });
        }
        const accountIds = books.flatMap((b) => [
            b.costAccount,
            b.accumulatedAccount,
            b.expenseAccount,
        ]), accounts = (await repository.bookAccounts(actor.companyId, accountIds)).results, byId = new Map(accounts.map((r) => [r.id, r]));
        if (accounts.length !== accountIds.length ||
            accounts.some((r) => r.currency !== a.currency ||
                r.base_currency !== a.currency ||
                r.chart_status !== "active" ||
                r.control_kind !== "none") ||
            books.some((b) => byId.get(b.costAccount)?.category !== "asset" ||
                byId.get(b.accumulatedAccount)?.category !== "asset" ||
                byId.get(b.expenseAccount)?.category !== "expense"))
            throw new BusinessRuleError(409, "ASSET_BOOK_ACCOUNT_CHANGED", "Use active same-currency noncontrol asset and expense accounts in every book.");
        return { books, rules, accounts };
    }
    async function edition(actor: WorkContext, id: string) {
        const row = await repository.edition(id, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "ASSET_POLICY_NOT_FOUND", "Asset depreciation policy not found.");
        return row;
    }
    async function postedHistory(a: Row) {
        const legacyCount = Number((await repository.legacyCount(a.id)) ?? 0);
        const rows = (await repository.postedRuns(a.id)).results;
        if (rows.some((r, i) => r.ordinal !== legacyCount + i + 1))
            throw new BusinessRuleError(409, "ASSET_POLICY_HISTORY_CHANGED", "Reconcile the posted depreciation sequence first.");
        return rows;
    }
    async function policyEvidence(actor: WorkContext, input: z.infer<typeof assetPolicySchema>) {
        const a = await asset(actor, input.assetId);
        if (a.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "ASSET_CHANGED", "Refresh the asset before preparing its policies.");
        const history = await postedHistory(a), legacyCount = Number((await repository.policyLegacyCount(a.id)) ?? 0), ordinal = legacyCount + history.length, date = input.effectiveDate ?? monthlyDate(String(a.in_service_date), ordinal), book = await bookEvidence(actor, a, input, date), parent = input.parentId ? await edition(actor, input.parentId) : null;
        if (parent &&
            (parent.status !== "active" ||
                parent.version !== input.parentVersion ||
                parent.asset_id !== a.id))
            throw new BusinessRuleError(409, "ASSET_POLICY_CHANGED", "Amend the active policy using its current version.");
        if (!parent &&
            (history.length ||
                ((legacyCount > 0 || Number(a.accumulated_depreciation_minor) !== 0) &&
                    !input.adoptLegacy)))
            throw new BusinessRuleError(409, "ASSET_POLICY_OPENING_REQUIRED", "Choose adoption of retained asset history and review its carrying basis before changing depreciation.");
        if (await repository.pendingRun(a.id))
            throw new BusinessRuleError(409, "ASSET_POLICY_RUN_PENDING", "Decide current depreciation drafts before revising policy.");
        if (date.slice(0, 7) !==
            monthlyDate(String(a.in_service_date), ordinal).slice(0, 7) ||
            date < String(a.in_service_date))
            throw new BusinessRuleError(422, "ASSET_POLICY_EFFECTIVE_DATE", "Apply the policy in the next unposted in-service month.");
        const carrying = await ports.carrying(a, book.books);
        if (ordinal > 0 && date < carrying.basis.throughDate)
            throw new BusinessRuleError(422, "ASSET_POLICY_EFFECTIVE_DATE", "Apply the revised policy after the retained depreciation history.");
        for (const b of input.books)
            if (b.policy.method === "units_of_production" &&
                b.policy.totalUnits <=
                    (carrying.basis.books.find((c) => c.chartId === b.chartId)?.units ?? 0))
                throw new BusinessRuleError(422, "ASSET_POLICY_USAGE_BASIS", "Reviewed lifetime units must include remaining usage beyond retained readings.");
        return {
            a,
            parent,
            basis: carrying.basis,
            hash: await ports.hash(JSON.stringify({ input, a, book, history, parent, carrying })),
        };
    }
    async function prepareAssetPolicy(actor: WorkContext, raw: z.input<typeof assetPolicySchema>, key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.write");
        const input = assetPolicySchema.parse(raw), id = `asset-policy-${key}`, op = "finance.asset-policy.prepare";
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, repository = ports.repository;
            const { policyEvidence } = createAssetPolicyApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            await ports.requireTool(actor.companyId, "asset_policies");
            const e = await policyEvidence(actor, input), now = ports.clock.now();
            return ports.commands.commit(actor, key, op, input, id, 1, { id, status: "draft", version: 1 }, [
                repository.preparePolicy(id, actor.companyId, input.assetId, e.parent?.id ?? null, JSON.stringify(input), e.hash, actor.userId, input.reason, now, now, JSON.stringify(e.basis)),
            ]);
        });
    }
    async function decideAssetPolicy(actor: WorkContext, id: string, raw: z.input<typeof assetPolicyDecisionSchema>, action: "approve" | "reject", key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.approve");
        const input = assetPolicyDecisionSchema.parse(raw), op = `finance.asset-policy.${action}`;
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, repository = ports.repository;
            const { edition, policyEvidence } = createAssetPolicyApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const row = await edition(actor, id);
            if (row.status !== "draft" || row.version !== input.expectedVersion)
                throw new BusinessRuleError(409, "ASSET_POLICY_CHANGED", "Refresh the policy draft before deciding.");
            if (samePerson(actor, row.prepared_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "Another Finance approver must review depreciation policy.");
            const e = action === "approve"
                ? await policyEvidence(actor, assetPolicySchema.parse(JSON.parse(String(row.content_json))))
                : null;
            if (e && e.hash !== row.source_hash)
                throw new BusinessRuleError(409, "ASSET_POLICY_SOURCE_CHANGED", "Asset or book evidence changed. Reject and prepare again.");
            const steps: Effect[] = e?.parent
                ? [
                    repository.supersedePolicy(ports.clock.now(), e.parent.id),
                ]
                : [];
            steps.push(repository.guardPolicyVersion(key, "asset-policy-version", id, actor.companyId, input.expectedVersion), repository.decidePolicy(action === "approve" ? "active" : "rejected", actor.userId, ports.clock.now(), id));
            return ports.commands.commit(actor, key, op, input, id, input.expectedVersion + 1, {
                id,
                status: action === "approve" ? "active" : "rejected",
                version: input.expectedVersion + 1,
            }, steps);
        });
    }
    async function runEvidence(actor: WorkContext, p: Row, input: z.infer<typeof assetPolicyRunSchema>) {
        if (!(await repository.openPeriod(input.periodId, actor.companyId, input.entryDate, input.entryDate)))
            throw new BusinessRuleError(409, "PERIOD_NOT_OPEN", "Use an open period covering the accounting date, including a zero-usage run.");
        if (p.status !== "active" || p.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "ASSET_POLICY_CHANGED", "Use the current active policy version.");
        const definition = assetPolicySchema.parse(JSON.parse(String(p.content_json))), a = await asset(actor, String(p.asset_id)), history = await postedHistory(a), book = await bookEvidence(actor, a, definition, input.entryDate), basis = p.basis_json
            ? (JSON.parse(String(p.basis_json)) as AssetCarryBasis)
            : {
                ordinal: 0,
                throughDate: String(a.in_service_date),
                books: definition.books.map((b) => ({
                    chartId: b.chartId,
                    accumulated: "0.00",
                    units: 0,
                })),
            }, legacyCount = Number((await repository.runLegacyCount(a.id)) ?? 0), ordinal = legacyCount + history.length + 1, carrying = await ports.carrying(a, book.books);
        const month = (d: string) => Number(d.slice(0, 4)) * 12 + Number(d.slice(5, 7));
        if (input.entryDate < String(a.in_service_date) ||
            month(input.depreciationMonth
                ? `${input.depreciationMonth}-01`
                : input.entryDate) !==
                month(String(a.in_service_date)) + ordinal - 1 ||
            (input.depreciationMonth &&
                input.entryDate.slice(0, 7) < input.depreciationMonth) ||
            input.entryDate <
                (definition.effectiveDate ??
                    monthlyDate(String(a.in_service_date), basis.ordinal)) ||
            input.entryDate < carrying.latestAccountingDate)
            throw new BusinessRuleError(422, "ASSET_DEPRECIATION_SEQUENCE", "Review each asset service month in order, using a current open posting date after retained source and reversal dates.");
        if (definition.books.some((b) => b.policy.method === "units_of_production") &&
            !input.usageEvidence)
            throw new BusinessRuleError(422, "ASSET_USAGE_EVIDENCE_REQUIRED", "Retain reviewed readings for units-of-production depreciation, including zero usage.");
        const books = definition.books.map((b) => {
            const previous = (history.at(-1) && Number(history.at(-1)!.ordinal) > basis.ordinal
                ? (JSON.parse(String(history.at(-1)!.result_json)) as Result).books.find((r) => r.chartId === b.chartId)
                : null) ?? basis.books.find((c) => c.chartId === b.chartId);
            if (history.length && !previous)
                throw new BusinessRuleError(409, "ASSET_BOOK_CARRYING_REQUIRED", "A new secondary book needs a reviewed asset carrying opening.");
            const accumulated = decimalToUnits(previous?.accumulated ?? "0.00"), units = previous?.units ?? 0, baseline = basis.books.find((c) => c.chartId === b.chartId);
            if (carrying.basis.books.find((c) => c.chartId === b.chartId)?.accumulated !==
                unitsToDecimal(accumulated))
                throw new BusinessRuleError(409, "ASSET_BOOK_CARRYING_CHANGED", "Book adjustments changed the asset carrying value. Review a new policy basis before depreciation.");
            const amount = depreciationAmount({
                cost: BigInt(a.cost_minor!),
                residual: BigInt(a.residual_minor!),
                life: Number(a.useful_life_months),
                ordinal,
                accumulated,
                policy: b.policy,
                priorUnits: units,
                units: input.units,
                basisOrdinal: basis.ordinal,
                basisAccumulated: decimalToUnits(baseline?.accumulated ?? "0.00"),
                basisUnits: baseline?.units ?? 0,
            });
            return {
                chartId: b.chartId,
                amount: unitsToDecimal(amount),
                accumulated: unitsToDecimal(accumulated + amount),
                units: units + input.units,
            };
        });
        const primary = books.find((b) => b.chartId === a.chart_id)!, previousPrimary = history.at(-1) && Number(history.at(-1)!.ordinal) > basis.ordinal
            ? (JSON.parse(String(history.at(-1)!.result_json)) as Result).books.find((b) => b.chartId === a.chart_id)!.accumulated
            : (basis.books.find((b) => b.chartId === a.chart_id)?.accumulated ??
                "0.00");
        if (BigInt(a.accumulated_depreciation_minor!) !==
            decimalToUnits(previousPrimary))
            throw new BusinessRuleError(409, "ASSET_HISTORY_RECONCILIATION_REQUIRED", "The asset carrying value differs from its retained policy history.");
        const result: Result = { ordinal, units: input.units, books };
        return {
            a,
            definition,
            book,
            result,
            primary,
            hash: await ports.hash(JSON.stringify({ p, input, a, history, book, result, carrying })),
        };
    }
    async function journals(actor: WorkContext, runId: string, input: z.infer<typeof assetPolicyRunSchema>, e: Awaited<ReturnType<typeof runEvidence>>, key: string, status: "draft" | "posted") {
        const output = [] as Array<{
            id: string;
            sourceType: "asset_policy_depreciation" | "book_adjustment";
            steps: Effect[];
        }>;
        for (const b of e.book.books) {
            const amount = decimalToUnits(e.result.books.find((r) => r.chartId === b.chartId)!.amount), delta = b.ruleId ? amount - decimalToUnits(e.primary.amount) : amount;
            if (delta === BigInt(0))
                continue;
            const positive = delta > BigInt(0), absolute = positive ? delta : -delta, sourceType = b.ruleId ? "book_adjustment" : "asset_policy_depreciation", lines: JournalLineInput[] = [
                {
                    accountId: b.expenseAccount,
                    debit: unitsToDecimal(positive ? absolute : BigInt(0)),
                    credit: unitsToDecimal(positive ? BigInt(0) : absolute),
                    dimensions: e.definition.dimensions,
                },
                {
                    accountId: b.accumulatedAccount,
                    debit: unitsToDecimal(positive ? BigInt(0) : absolute),
                    credit: unitsToDecimal(positive ? absolute : BigInt(0)),
                    dimensions: e.definition.dimensions,
                },
            ], j = await ports.prepareJournal(actor, {
                chartId: b.chartId,
                periodId: input.periodId,
                entryDate: input.entryDate,
                currency: String(e.a.currency),
                memo: input.reason,
                sourceType,
                sourceId: runId,
                isOpening: false,
                lines,
            }, `${key}:${output.length}`, status);
            output.push({ id: j.result.id, sourceType, steps: j.steps });
        }
        return output;
    }
    async function prepareAssetPolicyRun(actor: WorkContext, id: string, raw: z.input<typeof assetPolicyRunSchema>, key: string, ownedTransaction = false): Promise<{id:string;status:string;version:number;result:Result}> {
        actor = ports.requirePermission(actor, "finance.write");
        const input = assetPolicyRunSchema.parse(raw), runId = `asset-policy-run-${key}`, op = "finance.asset-policy-run.prepare";
        await ports.prepare(actor.holdingId);
        return ports.ownedTransaction(ownedTransaction, async (transactionPorts) => {
            const ports = transactionPorts, repository = ports.repository;
            const { edition, runEvidence, journals } = createAssetPolicyApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; result: Result; }>(actor, key, op, runId, { id, input });
            if (prior)
                return prior;
            await ports.requireTool(actor.companyId, "asset_policies");
            const p = await edition(actor, id), e = await runEvidence(actor, p, input);
            await journals(actor, runId, input, e, key, "draft");
            const now = ports.clock.now();
            return ports.commands.commit(actor, key, op, { id, input }, runId, 1, { id: runId, status: "draft", version: 1, result: e.result }, [
                repository.prepareRun(runId, actor.companyId, e.a.id, id, e.result.ordinal, input.entryDate, JSON.stringify(input), JSON.stringify(e.result), e.hash, actor.userId, input.reason, now, now),
            ]);
        });
    }
    async function getRun(actor: WorkContext, id: string) {
        const r = await repository.run(id, actor.companyId);
        if (!r)
            throw new BusinessRuleError(404, "ASSET_POLICY_RUN_NOT_FOUND", "Depreciation draft not found.");
        return r;
    }
    async function decideAssetPolicyRun(actor: WorkContext, id: string, raw: z.input<typeof assetPolicyDecisionSchema>, action: "post" | "reject", key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.approve");
        const input = assetPolicyDecisionSchema.parse(raw), op = `finance.asset-policy-run.${action}`;
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, repository = ports.repository;
            const { getRun, edition, runEvidence, journals } = createAssetPolicyApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; journalIds: string[]; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const r = await getRun(actor, id);
            if (r.status !== "draft" || r.version !== input.expectedVersion)
                throw new BusinessRuleError(409, "ASSET_POLICY_RUN_CHANGED", "Refresh the depreciation draft before deciding.");
            if (samePerson(actor, r.prepared_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "Another Finance approver must review depreciation.");
            const p = await edition(actor, String(r.policy_id)), definition = assetPolicyRunSchema.parse(JSON.parse(String(r.input_json))), e = action === "post" ? await runEvidence(actor, p, definition) : null;
            if (e && e.hash !== r.source_hash)
                throw new BusinessRuleError(409, "ASSET_POLICY_RUN_SOURCE_CHANGED", "Asset, policy or book evidence changed. Reject and prepare again.");
            const js = e
                ? await journals(actor, id, definition, e, key, "posted")
                : [], now = ports.clock.now(), steps: Effect[] = js.flatMap((j) => j.steps);
            if (e)
                steps.push(repository.updateAssetCarrying(String(decimalToUnits(e.primary.accumulated)), now, e.a.id), repository.updatePolicyVersion(now, p.id), repository.assetEvent(`asset-event-${key}`, e.a.id, actor.companyId, Number(e.a.version) + 1, key, JSON.stringify({
                    policyId: p.id,
                    runId: id,
                    ...e.result,
                    journalIds: js.map((j) => j.id),
                }), actor.userId, now));
            steps.push(repository.guardRunVersion(key, "asset-policy-run-version", id, actor.companyId, input.expectedVersion), repository.decideRun(action === "post" ? "posted" : "rejected", actor.userId, JSON.stringify(js.map((j) => ({ id: j.id, sourceType: j.sourceType }))), now, id));
            return ports.commands.commit(actor, key, op, input, id, input.expectedVersion + 1, {
                id,
                status: action === "post" ? "posted" : "rejected",
                version: input.expectedVersion + 1,
                journalIds: js.map((j) => j.id),
            }, steps);
        });
    }
    async function reverseAssetPolicyRun(actor: WorkContext, id: string, raw: z.input<typeof assetPolicyReversalSchema>, key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.approve");
        const input = assetPolicyReversalSchema.parse(raw), op = "finance.asset-policy-run.reverse";
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, repository = ports.repository;
            const { getRun, edition, asset } = createAssetPolicyApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; reversalJournalIds: string[]; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const r = await getRun(actor, id), p = await edition(actor, String(r.policy_id)), a = await asset(actor, String(r.asset_id));
            if (r.status !== "posted" ||
                r.version !== input.expectedVersion ||
                p.status !== "active")
                throw new BusinessRuleError(409, "ASSET_POLICY_RUN_CHANGED", "Reverse the latest depreciation in the current policy edition.");
            if (samePerson(actor, r.prepared_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "An independent Finance approver must reverse depreciation.");
            if ((await repository.laterRuns(a.id, r.ordinal)) ||
                (await repository.pendingPolicy(a.id)))
                throw new BusinessRuleError(409, "ASSET_POLICY_LATER_WORK", "Decide policy drafts and reverse later depreciation first.");
            const owned = JSON.parse(String(r.journal_ids_json)) as Array<{
                id: string;
                sourceType: "asset_policy_depreciation" | "book_adjustment";
            }>, reversals = [];
            for (const [i, j] of owned.entries())
                reversals.push(await ports.prepareReversal(actor, j.id, j.sourceType, id, input, `${key}:${i}`));
            const result = JSON.parse(String(r.result_json)) as Result, primary = result.books.find((b) => b.chartId === a.chart_id)!;
            return ports.commands.commit(actor, key, op, input, id, input.expectedVersion + 1, {
                id,
                status: "reversed",
                version: input.expectedVersion + 1,
                reversalJournalIds: reversals.map((j) => j.id),
            }, [
                ...reversals.flatMap((j) => j.steps),
                repository.reverseAssetCarrying(String(decimalToUnits(primary.amount)), ports.clock.now(), a.id),
                repository.reversePolicyVersion(ports.clock.now(), p.id),
                repository.reverseRun(JSON.stringify(reversals.map((j) => j.id)), input.entryDate, ports.clock.now(), id),
            ]);
        });
    }
    async function assetPolicyWorkspace(actor: WorkContext) {
        actor = ports.requirePermission(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const [policies, runs, assets, books, periods] = await Promise.all([
            repository.workspacePolicies(actor.companyId),
            repository.workspaceRuns(actor.companyId),
            repository.workspaceAssets(actor.companyId),
            repository.workspaceBooks(actor.companyId),
            repository.workspacePeriods(actor.companyId),
        ]);
        return {
            policies: policies.results,
            runs: runs.results,
            assets: assets.results,
            books: books.results,
            periods: periods.results,
        };
    }
    async function prepareAssetPolicyDisposal(actor: WorkContext, assetId: string, input: {
        id: string;
        periodId: string;
        entryDate: string;
        proceeds: number;
        gainAccountId: string;
        lossAccountId: string;
    }, key: string, status: "draft" | "posted") {
        const p = await repository.activeDisposalPolicy(assetId, actor.companyId);
        if (!p)
            return {
                hash: null,
                dimensions: {},
                steps: [] as Effect[],
                journalIds: [] as string[],
            };
        const a = await repository.disposalAsset(assetId, actor.companyId);
        if (!a)
            throw new BusinessRuleError(409, "ASSET_NOT_RECOGNIZED", "The asset carrying value is unavailable for disposal.");
        if ((await repository.disposalPendingRun(assetId)) ||
            (await repository.disposalPendingPolicy(assetId)))
            throw new BusinessRuleError(409, "ASSET_POLICY_PENDING", "Decide depreciation and policy drafts before disposal.");
        const definition = assetPolicySchema.parse(JSON.parse(String(p.content_json))), history = await postedHistory(a), book = await bookEvidence(actor, a, definition, input.entryDate), last = history.at(-1), carrying = last
            ? (JSON.parse(String(last.result_json)) as Result).books
            : p.basis_json
                ? (JSON.parse(String(p.basis_json)) as AssetCarryBasis).books
                : definition.books.map((b) => ({
                    chartId: b.chartId,
                    accumulated: "0.00",
                }));
        if (last && String(last.entry_date) > input.entryDate)
            throw new BusinessRuleError(422, "ASSET_DISPOSAL_DATE", "Disposal cannot precede retained book depreciation.");
        const primaryAccum = BigInt(a.accumulated_depreciation_minor!), primaryGain = BigInt(input.proceeds) - BigInt(a.cost_minor!) + primaryAccum, steps: Effect[] = [], journalIds: string[] = [], mappingEvidence = [];
        for (const b of book.books.filter((b) => b.ruleId)) {
            const retained = carrying.find((c) => c.chartId === b.chartId);
            if (!retained)
                throw new BusinessRuleError(409, "ASSET_BOOK_CARRYING_REQUIRED", "Review the asset's secondary-book carrying value before disposal.");
            const delta = decimalToUnits(retained.accumulated) - primaryAccum, secondaryGain = primaryGain + delta;
            const maps = (await repository.disposalBookMappings(b.ruleId, input.gainAccountId, input.lossAccountId)).results, byId = new Map(maps.map((m) => [m.primary_account_id, String(m.secondary_account_id)]));
            if (maps.length !== 2 || maps.some((m) => m.status !== "active"))
                throw new BusinessRuleError(409, "ASSET_DISPOSAL_BOOK_MAPPING_REQUIRED", "Map the disposal gain and loss accounts in each book.");
            mappingEvidence.push(maps);
            const balances = new Map<string, bigint>();
            function add(id: string, value: bigint) {
                balances.set(id, (balances.get(id) ?? BigInt(0)) + value);
            }
            add(b.accumulatedAccount, delta);
            if (primaryGain > BigInt(0))
                add(byId.get(input.gainAccountId)!, primaryGain);
            else if (primaryGain < BigInt(0))
                add(byId.get(input.lossAccountId)!, primaryGain);
            if (secondaryGain > BigInt(0))
                add(byId.get(input.gainAccountId)!, -secondaryGain);
            else if (secondaryGain < BigInt(0))
                add(byId.get(input.lossAccountId)!, -secondaryGain);
            const lines = [...balances]
                .filter(([, amount]) => amount !== BigInt(0))
                .map(([accountId, amount]) => ({
                accountId,
                debit: unitsToDecimal(amount > BigInt(0) ? amount : BigInt(0)),
                credit: unitsToDecimal(amount < BigInt(0) ? -amount : BigInt(0)),
                dimensions: definition.dimensions,
            }));
            if (!lines.length)
                continue;
            const journal = await ports.prepareJournal(actor, {
                chartId: b.chartId,
                periodId: input.periodId,
                entryDate: input.entryDate,
                currency: String(a.currency),
                memo: "Retire secondary-book asset carrying value",
                sourceType: "book_adjustment",
                sourceId: input.id,
                isOpening: false,
                lines,
            }, `${key}:disposal:${journalIds.length}`, status);
            steps.push(...journal.steps);
            journalIds.push(journal.result.id);
        }
        return {
            hash: await ports.hash(JSON.stringify({ p, a, book, history, mappingEvidence, input })),
            dimensions: definition.dimensions,
            steps,
            journalIds,
        };
    }
    return { asset, bookEvidence, edition, postedHistory, policyEvidence, runEvidence, journals, getRun, prepareAssetPolicy, decideAssetPolicy, prepareAssetPolicyRun, decideAssetPolicyRun, reverseAssetPolicyRun, assetPolicyWorkspace, prepareAssetPolicyDisposal };
}


