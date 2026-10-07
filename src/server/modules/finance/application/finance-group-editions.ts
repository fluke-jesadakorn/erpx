import { convert } from '../domain/group-consolidation-math';
import type { intercompanyPairSchema } from '../public/finance-intercompany-pairs';
import { z } from "zod";
import { decimalToUnits, unitsToDecimal } from "@/lib/finance-money";
import { calculateFinance } from "../domain/finance-calculations";
import { financeCsv as serializeFinanceCsv } from "@/lib/finance-csv";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { capturedFiscalClose } from "@/server/modules/finance/public/finance-fiscal-close-source";
import type { requireFinanceTool } from "@/server/modules/finance/public/finance-company-tools";
import type { requireFinanceMoneyScale } from "@/server/modules/finance/public/finance-money-policy";
import type { sha256 } from "@/server/modules/runtime/public/crypto";
import type { groupOwnershipWorkspace } from "@/server/modules/finance/public/finance-group-ownership";
import type { intercompanyPairWorkspace } from "@/server/modules/finance/public/finance-intercompany-pairs";
import type { intercompanyPairSource } from "@/server/modules/finance/public/finance-intercompany-pairs";
import { type Row, groupAccount, groupReportEditionSchema, groupReportDecisionSchema, type Input, profitCategory } from '../domain/finance-group-editions';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { GroupCommands } from '../domain/group-command-ports';
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
export interface FinanceGroupEditionsRepository<Effect> {
    readSourceRegistration(holdingId: string, companyId: string, chartId: string): Promise<Row | null>;
    readSourcePeriods(companyId: string, periodIds: string[], entity: {
        companyId: string;
        chartId: string;
        periodIds: string[];
        method: "full" | "equity";
    }): Promise<{
        results: (Row)[];
    }>;
    readSourceLines(companyId: string, chartId: string, endDate: string): Promise<{
        results: (Row)[];
    }>;
    readSourcePair(id: string, holdingId: string): Promise<Row | null>;
    readSourceAcquisition(acquisitionId: string, holdingId: string, companyId: string, currency: string, endDate: string): Promise<Row | null>;
    readSourceOwnership(ownership_id: string | number, holdingId: string): Promise<Row | null>;
    readSourceCalc(calculation_run_id: string | number | null, holdingId: string, companyId: string): Promise<Row | null>;
    readSourceCalc2(calculationRunId: string, holdingId: string, companyId: string, currency: string): Promise<Row | null>;
    readSourceOwnership2(ownershipId: string, holdingId: string, startDate: string, endDate: string): Promise<Row | null>;
    readSourceEffect(value: string, holdingId: string, investee_entity_id: string | number | null, endDate: string, currency: string, id: string | number | null): Promise<Row | null>;
    readGetR(id: string, holdingId: string, companyId: string): Promise<Row | null>;
    stagePrepareGroupReportEditionEffect(id: string, holdingId: string, companyId: string, value: string | number | null, value2: string | number, value3: number, startDate: string, endDate: string, currency: string, value4: string, value5: string, value6: string, hash: string, csv: string, exportHash: string, userId: string, reason: string, now: number, now2: number): Effect;
    stageDecideGroupReportEditionEffect(now: number, id: string | number | null): Effect;
    guard14(key: string, value: "group-edition-version", id: string, holdingId: string, expectedVersion: number): Effect;
    stageDecideGroupReportEditionEffect2(value: "approved" | "rejected", userId: string, now: number, id: string): Effect;
    readGroupReportEditionWorkspaceEffect(holdingId: string, companyId: string): Promise<{
        results: (Row)[];
    }>;
    readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations(holdingId: string): Promise<{
        results: (Row)[];
    }>;
    readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations2(holdingId: string): Promise<{
        results: (Row)[];
    }>;
    readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations3(holdingId: string): Promise<{
        results: (Row)[];
    }>;
    readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations4(holdingId: string): Promise<{
        results: (Row)[];
    }>;
    readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations5(holdingId: string, companyId: string): Promise<{
        results: (Row)[];
    }>;
    readGroupRecordNamesPeople(companyId: string, ids: string[]): Promise<{
        results: ({
            user_id: string;
            legal_name: string;
        })[];
    }>;
    readGroupExpansionRecordRecord(holdingId: string, companyId: string, id: string, table: string): Promise<Row | null>;
}
export interface FinanceGroupEditionsPorts<Effect> {
    repository: FinanceGroupEditionsRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: GroupCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    batch(effects: Effect[]): Promise<unknown>;
    transaction<Result>(run: (ports: FinanceGroupEditionsPorts<Effect>) => Promise<Result>): Promise<Result>;
    digestBytes(bytes: Uint8Array<ArrayBuffer>): Promise<ArrayBuffer>;
    parseIntercompanyPair(input: unknown): z.output<typeof intercompanyPairSchema>;
    capturedFiscalClose(...args: Parameters<typeof capturedFiscalClose>): PortResult<ReturnType<typeof capturedFiscalClose>, Effect>;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
    requireFinanceTool(...args: Tail<Parameters<typeof requireFinanceTool>>): PortResult<ReturnType<typeof requireFinanceTool>, Effect>;
    requireFinanceMoneyScale(...args: Tail<Parameters<typeof requireFinanceMoneyScale>>): PortResult<ReturnType<typeof requireFinanceMoneyScale>, Effect>;
    sha256(...args: Parameters<typeof sha256>): PortResult<ReturnType<typeof sha256>, Effect>;
    groupOwnershipWorkspace(...args: Parameters<typeof groupOwnershipWorkspace>): PortResult<ReturnType<typeof groupOwnershipWorkspace>, Effect>;
    intercompanyPairWorkspace(...args: Parameters<typeof intercompanyPairWorkspace>): PortResult<ReturnType<typeof intercompanyPairWorkspace>, Effect>;
    intercompanyPairSource(...args: Tail<Parameters<typeof intercompanyPairSource>>): PortResult<ReturnType<typeof intercompanyPairSource>, Effect>;
}
export function createFinanceGroupEditionsApplication<Effect>(ports: FinanceGroupEditionsPorts<Effect>) {
    const repository = ports.repository;
    function holdingActor(actor: WorkContext, permission: string) {
        actor = ports.requirePermission(actor, permission);
        if (actor.scope.kind !== "holding")
            throw new BusinessRuleError(403, "HOLDING_FINANCE_REQUIRED", "Group reporting requires Holding Finance authority.");
        return actor;
    }
    async function source(actor: WorkContext, input: Input) {
        await ports.requireFinanceMoneyScale(actor.companyId, input.currency);
        const definitions = new Map<string, z.infer<typeof groupAccount>>(), cumulative = new Map<string, bigint>(), interval = new Map<string, bigint>(), entities = [], adjustments = [], eliminations = [], acquisitions = [];
        let nciProfitAttribution = BigInt(0);
        function register(g: z.infer<typeof groupAccount>) {
            const old = definitions.get(g.code);
            if (old && JSON.stringify(old) !== JSON.stringify(g))
                throw new BusinessRuleError(422, "GROUP_ACCOUNT_CONFLICT", "Every group code must retain the same name and category.");
            definitions.set(g.code, g);
        }
        function add(map: Map<string, bigint>, g: z.infer<typeof groupAccount>, amount: bigint) {
            register(g);
            map.set(g.code, (map.get(g.code) ?? BigInt(0)) + amount);
        }
        const accountMappings = new Map(input.mappings.map((m) => [m.accountId, m]));
        for (const entity of input.entities) {
            const registration = await repository.readSourceRegistration(actor.holdingId, entity.companyId, entity.chartId);
            if (!registration ||
                registration.chart_status !== "active" ||
                registration.functional_currency !== registration.base_currency)
                throw new BusinessRuleError(422, "GROUP_BOOK_REQUIRED", "Select an active registered Company book with its registered functional currency.");
            await ports.requireFinanceMoneyScale(entity.companyId, String(registration.base_currency));
            const periods = (await repository.readSourcePeriods(entity.companyId, entity.periodIds, entity)).results;
            if (periods.length !== entity.periodIds.length ||
                periods.some((p) => p.state !== "closed") ||
                periods[0]?.start_date !== input.startDate ||
                periods.at(-1)?.end_date !== input.endDate ||
                periods.some((p, i) => i > 0 &&
                    new Date(Date.parse(String(periods[i - 1].end_date)) + 86400000)
                        .toISOString()
                        .slice(0, 10) !== p.start_date))
                throw new BusinessRuleError(409, "GROUP_PERIODS_NOT_ALIGNED", "Every Company's closed periods must cover the exact reporting interval without gaps or overlaps.");
            const lines = (await repository.readSourceLines(entity.companyId, entity.chartId, input.endDate)).results.map(({ is_fiscal_close, ...line }) => Number(is_fiscal_close) === 1 && line.source_type === "book_projection"
                ? { ...line, is_fiscal_close: 1 }
                : line);
            if (lines.length > 100000)
                throw new BusinessRuleError(422, "GROUP_SOURCE_SIZE", "This book exceeds the current retained-source limit. Split the source adapter before preparing this edition.");
            const functional = String(registration.base_currency), fx = input.rates.find((r) => r.currency === functional), byAccount = new Map<string, {
                cumulative: bigint;
                interval: bigint;
                category: string;
            }>();
            for (const l of lines) {
                const m = accountMappings.get(String(l.account_id));
                if (!m || m.group.category !== l.category)
                    throw new BusinessRuleError(422, "GROUP_MAPPING_REQUIRED", "Map every retained posted account to the matching group category.");
                let exchange = BigInt(1000000);
                if (functional !== input.currency) {
                    const selected = m.fxType === "historical" ? m.historicalRate : fx?.[m.fxType];
                    if (!selected)
                        throw new BusinessRuleError(422, "GROUP_RATE_REQUIRED", "Provide Current, Average and mapped Historical rates for each foreign source.");
                    exchange = decimalToUnits(selected, 6);
                }
                const raw = BigInt(l.base_debit_minor!) - BigInt(l.base_credit_minor!), amount = convert(raw, exchange), previous = byAccount.get(String(l.account_id)) ?? {
                    cumulative: BigInt(0),
                    interval: BigInt(0),
                    category: String(l.category),
                };
                previous.cumulative += amount;
                if (String(l.entry_date) >= input.startDate &&
                    !ports.capturedFiscalClose(l))
                    previous.interval += amount;
                byAccount.set(String(l.account_id), previous);
            }
            const rawBalance = lines.reduce((s, l) => s + BigInt(l.base_debit_minor!) - BigInt(l.base_credit_minor!), BigInt(0));
            if (rawBalance !== BigInt(0))
                throw new BusinessRuleError(409, "GROUP_SOURCE_UNBALANCED", "The selected Company book does not balance at the cutoff.");
            const translated = [...byAccount.values()].reduce((s, a) => s + a.cumulative, BigInt(0)), profit = -[...byAccount.values()]
                .filter((a) => profitCategory(a.category))
                .reduce((s, a) => s + a.interval, BigInt(0));
            if (entity.method === "full") {
                for (const [id, b] of byAccount) {
                    const m = accountMappings.get(id)!;
                    if (profitCategory(b.category))
                        add(interval, m.group, b.interval);
                    else
                        add(cumulative, m.group, b.cumulative);
                }
                const openProfit = [...byAccount.values()]
                    .filter((a) => profitCategory(a.category))
                    .reduce((s, a) => s + a.cumulative, BigInt(0));
                add(cumulative, {
                    code: "_unclosed_profit",
                    name: "Unclosed Company profit",
                    category: "equity",
                }, openProfit);
                add(cumulative, {
                    code: "_cta",
                    name: "Currency translation adjustment",
                    category: "equity",
                }, -translated);
            }
            entities.push({
                definition: entity,
                registration,
                periods,
                lines,
                profit: unitsToDecimal(profit),
                translatedUnbalance: unitsToDecimal(translated),
                accounts: [...byAccount].map(([accountId, b]) => ({
                    accountId,
                    category: b.category,
                    cumulative: unitsToDecimal(b.cumulative),
                    interval: unitsToDecimal(b.interval),
                })),
            });
        }
        if (new Set(input.intercompanyPairIds).size !== input.intercompanyPairIds.length)
            throw new BusinessRuleError(422, "GROUP_INTERCOMPANY_DUPLICATE", "Select each confirmed document pair once.");
        for (const id of input.intercompanyPairIds) {
            const pair = await repository.readSourcePair(id, actor.holdingId);
            if (!pair)
                throw new BusinessRuleError(409, "GROUP_INTERCOMPANY_NOT_CONFIRMED", "Use independently confirmed intercompany document pairs.");
            const definition = ports.parseIntercompanyPair(JSON.parse(String(pair.source_json)).input), proof = await ports.intercompanyPairSource(actor, definition);
            if (proof.hash !== pair.source_hash)
                throw new BusinessRuleError(409, "GROUP_INTERCOMPANY_SOURCE_CHANGED", "A paired document changed. Reverse the pairing and review new evidence.");
            const lines = [];
            for (const [i, s] of [definition.source, definition.target].entries()) {
                const e = entities.find((e) => e.definition.companyId === s.companyId), journal = proof.evidence[i].journal, m = accountMappings.get(s.accountId);
                if (!e ||
                    e.definition.method !== "full" ||
                    e.definition.chartId !== journal.chart_id ||
                    !m ||
                    String(journal.entry_date) > input.endDate)
                    throw new BusinessRuleError(409, "GROUP_INTERCOMPANY_BOOK_REQUIRED", "Both paired documents must belong to selected full-consolidation books at the reporting cutoff.");
                if (profitCategory(m.group.category) &&
                    String(journal.entry_date) < input.startDate)
                    throw new BusinessRuleError(409, "GROUP_INTERCOMPANY_INTERVAL_REQUIRED", "Income and expense document pairs must belong to this reporting interval.");
                const functional = String(e.registration.base_currency), rate = input.rates.find((r) => r.currency === functional), fx = functional === input.currency
                    ? BigInt(1000000)
                    : decimalToUnits((m.fxType === "historical"
                        ? m.historicalRate
                        : rate?.[m.fxType])!, 6), amount = -proof.evidence[i].lines.reduce((sum, l) => sum +
                    convert(BigInt(l.base_debit_minor!) - BigInt(l.base_credit_minor!), fx), BigInt(0));
                lines.push({ account: m.group, amount });
            }
            if (lines.reduce((s, l) => s + l.amount, BigInt(0)) !== BigInt(0))
                throw new BusinessRuleError(409, "GROUP_INTERCOMPANY_TRANSLATION_DIFFERENCE", "Paired translated amounts differ. Reconcile the Company FX or mapped translation policy before eliminating.");
            for (const l of lines)
                add(profitCategory(l.account.category) ? interval : cumulative, l.account, l.amount);
            eliminations.push({
                pair,
                proof: proof.evidence,
                lines: lines.map((l) => ({
                    account: l.account,
                    amount: unitsToDecimal(l.amount),
                })),
            });
        }
        if (new Set(input.acquisitionEliminations.map((a) => a.acquisitionId)).size !==
            input.acquisitionEliminations.length)
            throw new BusinessRuleError(422, "GROUP_ACQUISITION_DUPLICATE", "Select each acquisition elimination once.");
        for (const a of input.acquisitionEliminations) {
            const acquisition = await repository.readSourceAcquisition(a.acquisitionId, actor.holdingId, actor.companyId, input.currency, input.endDate);
            if (!acquisition || !acquisition.ownership_id)
                throw new BusinessRuleError(409, "GROUP_ACQUISITION_SOURCE_REQUIRED", "Select approved owned acquisition evidence in the presentation currency.");
            const ownership = await repository.readSourceOwnership(acquisition.ownership_id, actor.holdingId), investee = entities.find((e) => e.definition.companyId === ownership?.investee_company), investor = entities.find((e) => e.definition.companyId === ownership?.investor_company), mapping = accountMappings.get(a.investmentAccountId);
            if (!ownership ||
                !investee ||
                !investor ||
                investee.definition.method !== "full" ||
                investor.definition.method !== "full" ||
                String(ownership.effective_from) > String(acquisition.acquisition_date) ||
                (ownership.effective_to &&
                    String(ownership.effective_to) <
                        String(acquisition.acquisition_date)) ||
                !mapping ||
                mapping.group.category !== "asset" ||
                !investor.accounts.some((b) => b.accountId === a.investmentAccountId) ||
                a.goodwill.category !== "asset" ||
                a.nci.category !== "equity" ||
                a.bargainPurchase.category !== "other_income")
                throw new BusinessRuleError(409, "GROUP_ACQUISITION_ACCOUNTS_REQUIRED", "Bind an investor investment asset, subsidiary equity, goodwill asset, NCI equity and bargain-purchase income.");
            if (new Set([
                mapping.group.code,
                a.goodwill.code,
                a.nci.code,
                a.bargainPurchase.code,
            ]).size !== 4)
                throw new BusinessRuleError(422, "GROUP_ACQUISITION_ACCOUNT_CONFLICT", "Keep investment, goodwill, NCI and bargain-purchase group accounts distinct.");
            const consideration = BigInt(acquisition.consideration_minor!) +
                BigInt(acquisition.previous_interest_minor!), investorCarrying = decimalToUnits(investor.accounts.find((b) => b.accountId === a.investmentAccountId)!
                .cumulative);
            if (investorCarrying !== consideration)
                throw new BusinessRuleError(409, "GROUP_INVESTMENT_CARRYING_MISMATCH", "The selected investor investment carrying balance must match reviewed consideration and previous interest.");
            const functional = String(investee.registration.base_currency), rates = input.rates.find((r) => r.currency === functional), equityAtAcquisition = new Map<string, bigint>();
            for (const l of investee.lines.filter((l) => String(l.entry_date) <= String(acquisition.acquisition_date))) {
                const m = accountMappings.get(String(l.account_id))!;
                if (l.category !== "equity" && !profitCategory(String(l.category)))
                    continue;
                const fx = functional === input.currency
                    ? BigInt(1000000)
                    : decimalToUnits((m.fxType === "historical"
                        ? m.historicalRate
                        : rates?.[m.fxType])!, 6), amount = convert(BigInt(l.base_debit_minor!) - BigInt(l.base_credit_minor!), fx), code = l.category === "equity" ? m.group.code : "_unclosed_profit";
                equityAtAcquisition.set(code, (equityAtAcquisition.get(code) ?? BigInt(0)) + amount);
            }
            const net = BigInt(acquisition.identifiable_net_assets_minor!) -
                BigInt(acquisition.deferred_tax_minor!), sourceNet = -[...equityAtAcquisition.values()].reduce((s, v) => s + v, BigInt(0));
            if (sourceNet !== net)
                throw new BusinessRuleError(409, "GROUP_ACQUISITION_EQUITY_MISMATCH", "Reviewed identifiable net assets after deferred tax must reconcile to the translated subsidiary equity at acquisition. Review fair-value adjustments before preparing.");
            const calc = await repository.readSourceCalc(acquisition.calculation_run_id, actor.holdingId, actor.companyId);
            if (!calc)
                throw new BusinessRuleError(409, "GROUP_ACQUISITION_CALCULATION_REQUIRED", "The acquisition's owned calculation must remain approved.");
            const data = JSON.parse(String(calc.input_json)), result = calculateFinance("consolidation", data, 2), hash = await ports.sha256(JSON.stringify({
                kind: "consolidation",
                input: data,
                policyId: calc.policy_id ?? null,
                policyVersion: Number(calc.policy_version),
                currency: calc.currency,
                scale: Number(calc.currency_scale),
            }));
            if (hash !== calc.source_hash ||
                JSON.stringify(result) !== calc.result_json ||
                Number(data.parentBps) !== Number(ownership.ownership_bps) ||
                decimalToUnits(String(data.consideration)) !==
                    BigInt(acquisition.consideration_minor!) ||
                decimalToUnits(String(data.previousInterest)) !==
                    BigInt(acquisition.previous_interest_minor!) ||
                decimalToUnits(String(data.fairValueNetAssets)) !== net ||
                decimalToUnits(String(data.nciFairValue)) !==
                    BigInt(acquisition.nci_minor!))
                throw new BusinessRuleError(409, "GROUP_ACQUISITION_CALCULATION_CHANGED", "The acquisition calculation must match all retained reviewed inputs.");
            const goodwill = decimalToUnits(String(result.goodwill)), bargain = decimalToUnits(String(result.bargainPurchase));
            if (goodwill !== BigInt(acquisition.goodwill_minor!) ||
                bargain !== BigInt(acquisition.bargain_purchase_minor!))
                throw new BusinessRuleError(409, "GROUP_ACQUISITION_CALCULATION_CHANGED", "Goodwill or bargain-purchase outputs changed.");
            const postings = [];
            for (const [code, amount] of equityAtAcquisition) {
                const account = definitions.get(code)!;
                add(cumulative, account, -amount);
                postings.push({ account, amount: unitsToDecimal(-amount) });
            }
            add(cumulative, mapping.group, -consideration);
            add(cumulative, a.goodwill, goodwill);
            add(cumulative, a.nci, -BigInt(acquisition.nci_minor!));
            add(interval, a.bargainPurchase, -bargain);
            add(cumulative, {
                code: "_unclosed_profit",
                name: "Unclosed Company profit",
                category: "equity",
            }, -bargain);
            acquisitions.push({
                definition: a,
                acquisition,
                ownership,
                calculation: calc,
                postings: [
                    ...postings,
                    { account: mapping.group, amount: unitsToDecimal(-consideration) },
                    { account: a.goodwill, amount: unitsToDecimal(goodwill) },
                    {
                        account: a.nci,
                        amount: unitsToDecimal(-BigInt(acquisition.nci_minor!)),
                    },
                    { account: a.bargainPurchase, amount: unitsToDecimal(-bargain) },
                ],
            });
        }
        for (const a of input.adjustments) {
            if (["goodwill", "bargainPurchase"].includes(a.output))
                throw new BusinessRuleError(422, "GROUP_ACQUISITION_ELIMINATION_REQUIRED", "Use a complete acquisition elimination to remove investment and acquisition-date subsidiary equity together with goodwill and NCI.");
            const calc = await repository.readSourceCalc2(a.calculationRunId, actor.holdingId, actor.companyId, input.currency), ownership = await repository.readSourceOwnership2(a.ownershipId, actor.holdingId, input.startDate, input.endDate);
            if (!calc || !ownership)
                throw new BusinessRuleError(409, "GROUP_OWNERSHIP_CALCULATION_REQUIRED", "Use an approved owned consolidation calculation and effective approved ownership evidence.");
            const data = JSON.parse(String(calc.input_json)) as Record<string, unknown>, result = calculateFinance("consolidation", data, 2), sourceEntity = entities.find((e) => e.definition.companyId === ownership.investee_company);
            if (!sourceEntity ||
                !entities.some((e) => e.definition.companyId === ownership.investor_company) ||
                JSON.stringify(result) !== calc.result_json)
                throw new BusinessRuleError(409, "GROUP_CALCULATION_CHANGED", "The calculation must reproduce its retained result and reference selected entities.");
            const calcHash = await ports.sha256(JSON.stringify({
                kind: "consolidation",
                input: data,
                policyId: calc.policy_id ?? null,
                policyVersion: Number(calc.policy_version),
                currency: calc.currency,
                scale: Number(calc.currency_scale),
            }));
            if (calcHash !== calc.source_hash)
                throw new BusinessRuleError(409, "GROUP_CALCULATION_CHANGED", "The approved calculation input no longer matches its retained checksum.");
            const equity = a.output === "equityMethodClosing", acquisitionOutput = ["goodwill", "bargainPurchase"].includes(a.output);
            if (ownership.accounting_method !== (equity ? "equity" : "subsidiary") ||
                Number(data[equity ? "associateBps" : "parentBps"]) !==
                    Number(ownership.ownership_bps) ||
                (!acquisitionOutput &&
                    decimalToUnits(String(data[equity ? "associateProfit" : "subsidiaryProfit"])) !== decimalToUnits(sourceEntity.profit)) ||
                sourceEntity.definition.method !== (equity ? "equity" : "full"))
                throw new BusinessRuleError(409, "GROUP_CALCULATION_SOURCE_MISMATCH", "Approved ownership percentage and actual translated Company profit must match the calculation inputs.");
            let acquisition: Row | null = null;
            if (["goodwill", "bargainPurchase"].includes(a.output)) {
                acquisition = await repository.readSourceEffect(a.acquisitionId ?? "", actor.holdingId, ownership.investee_entity_id, input.endDate, input.currency, calc.id);
                if (!acquisition ||
                    acquisition.ownership_id !== ownership.id ||
                    decimalToUnits(String(data.consideration)) !==
                        BigInt(acquisition.consideration_minor!) ||
                    decimalToUnits(String(data.previousInterest ?? "0")) !==
                        BigInt(acquisition.previous_interest_minor!) ||
                    decimalToUnits(String(data.fairValueNetAssets)) !==
                        BigInt(acquisition.identifiable_net_assets_minor!) -
                            BigInt(acquisition.deferred_tax_minor!) ||
                    decimalToUnits(String(data.nciFairValue)) !==
                        BigInt(acquisition.nci_minor!) ||
                    decimalToUnits(String(result.goodwill)) !==
                        BigInt(acquisition.goodwill_minor!) ||
                    decimalToUnits(String(result.bargainPurchase)) !==
                        BigInt(acquisition.bargain_purchase_minor!))
                    throw new BusinessRuleError(409, "GROUP_ACQUISITION_SOURCE_MISMATCH", "Bind goodwill to the approved acquisition and its exact reviewed calculation.");
            }
            if (a.output === "nciProfitShare") {
                nciProfitAttribution += decimalToUnits(String(result.nciProfitShare));
                adjustments.push({
                    definition: a,
                    calculation: calc,
                    ownership,
                    acquisition,
                    amount: String(result.nciProfitShare),
                });
                continue;
            }
            const existing = decimalToUnits(a.existingDebitBalance), actual = a.output === "nciClosing"
                ? -(cumulative.get(a.credit.code) ?? BigInt(0))
                : (cumulative.get(a.debit.code) ?? BigInt(0));
            if (existing !== actual)
                throw new BusinessRuleError(409, "GROUP_ADJUSTMENT_CARRYING_CHANGED", "The retained equity-method debit or NCI credit carrying balance must match the selected consolidated source.");
            if (a.output === "nciClosing" &&
                (a.debit.category !== "equity" || a.credit.category !== "equity"))
                throw new BusinessRuleError(422, "GROUP_NCI_ACCOUNTS_REQUIRED", "NCI closing reallocates parent and noncontrolling equity without changing total group profit.");
            const amount = decimalToUnits(String(result[a.output])) - existing;
            if (a.debit.code === a.credit.code)
                throw new BusinessRuleError(422, "GROUP_ADJUSTMENT_ACCOUNT_PAIR", "Choose distinct group adjustment accounts.");
            const destination = profitCategory(a.debit.category) || profitCategory(a.credit.category)
                ? interval
                : cumulative;
            // A mixed BS/P&L adjustment also carries the P&L result into unclosed equity.
            add(profitCategory(a.debit.category) ? interval : cumulative, a.debit, amount);
            add(profitCategory(a.credit.category) ? interval : cumulative, a.credit, -amount);
            if (destination === interval) {
                const pl = (profitCategory(a.debit.category) ? amount : BigInt(0)) -
                    (profitCategory(a.credit.category) ? amount : BigInt(0));
                add(cumulative, {
                    code: "_unclosed_profit",
                    name: "Unclosed Company profit",
                    category: "equity",
                }, pl);
            }
            adjustments.push({
                definition: a,
                calculation: calc,
                ownership,
                acquisition,
                amount: unitsToDecimal(amount),
            });
        }
        for (const e of entities.filter((e) => e.definition.method === "equity"))
            if (!adjustments.some((a) => a.ownership.investee_company === e.definition.companyId &&
                a.definition.output === "equityMethodClosing"))
                throw new BusinessRuleError(409, "GROUP_EQUITY_ADJUSTMENT_REQUIRED", "An excluded associate book requires its owned equity-method closing adjustment.");
        if ([...cumulative.values()].reduce((s, a) => s + a, BigInt(0)) !== BigInt(0))
            throw new BusinessRuleError(409, "GROUP_BALANCE_SHEET_UNBALANCED", "Group balance-sheet sources and reviewed adjustments must balance.");
        const profit = -[...interval.values()].reduce((s, a) => s + a, BigInt(0)), moneyRows = (rows: Map<string, bigint>) => [...rows]
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([code, amount]) => ({
            ...definitions.get(code)!,
            amount: unitsToDecimal(amount),
        })), result = {
            currency: input.currency,
            startDate: input.startDate,
            endDate: input.endDate,
            balanceSheet: moneyRows(cumulative),
            profitAndLoss: moneyRows(interval),
            profit: unitsToDecimal(profit),
            profitAttribution: {
                parent: unitsToDecimal(profit - nciProfitAttribution),
                nci: unitsToDecimal(nciProfitAttribution),
            },
        };
        const evidence = { entities, adjustments, eliminations, acquisitions }, hash = await ports.sha256(JSON.stringify({ input, evidence, result }));
        return { evidence, result, hash };
    }
    async function get(actor: WorkContext, id: string) {
        const r = await repository.readGetR(id, actor.holdingId, actor.companyId);
        if (!r)
            throw new BusinessRuleError(404, "GROUP_EDITION_NOT_FOUND", "Group reporting edition not found.");
        return r;
    }
    async function prepareGroupReportEdition(actor: WorkContext, raw: z.input<typeof groupReportEditionSchema>, key: string): Promise<Record<string, unknown>> {
        actor = holdingActor(actor, "finance.write");
        const input = groupReportEditionSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        const id = `group-edition-${key}`;
        const op = "finance.group-edition.prepare";
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts;
            const repository = ports.repository;
            const { source, get } = createFinanceGroupEditionsApplication(ports).internal;
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; sourceHash: string; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            await ports.requireFinanceTool(actor.companyId, "group_reporting");
            const parent = input.parentId ? await get(actor, input.parentId) : null;
            if (parent &&
                (parent.status !== "approved" ||
                    parent.version !== input.parentVersion ||
                    parent.company_id !== actor.companyId ||
                    parent.start_date !== input.startDate ||
                    parent.end_date !== input.endDate ||
                    parent.currency !== input.currency))
                throw new BusinessRuleError(409, "GROUP_EDITION_PARENT_CHANGED", "Amend the current approved edition for the same report interval and presentation currency.");
            const e = await source(actor, input), csv = serializeFinanceCsv(["statement", "groupCode", "name", "category", "amount", "currency"], [
                ...e.result.balanceSheet.map((r) => [
                    "balance_sheet",
                    r.code,
                    r.name,
                    r.category,
                    r.amount,
                    input.currency,
                ]),
                ...e.result.profitAndLoss.map((r) => [
                    "profit_and_loss",
                    r.code,
                    r.name,
                    r.category,
                    r.amount,
                    input.currency,
                ]),
                ["profit_attribution", "parent", "Profit attributable to parent", "income", e.result.profitAttribution.parent, input.currency],
                ["profit_attribution", "nci", "Profit attributable to non-controlling interests", "income", e.result.profitAttribution.nci, input.currency],
            ]), exportHash = await ports.sha256(csv), now = ports.clock.now();
            return ports.commands.commit(actor, key, op, input, id, 1, { id, status: "draft", version: 1, sourceHash: e.hash }, [
                repository.stagePrepareGroupReportEditionEffect(id, actor.holdingId, actor.companyId, parent?.id ?? null, parent?.root_id ?? id, Number(parent?.edition ?? 0) + 1, input.startDate, input.endDate, input.currency, JSON.stringify(input), JSON.stringify(e.evidence), JSON.stringify(e.result), e.hash, csv, exportHash, actor.userId, input.reason, now, now),
            ]);
        });
    }
    async function decideGroupReportEdition(actor: WorkContext, id: string, raw: z.input<typeof groupReportDecisionSchema>, action: "approve" | "reject", key: string): Promise<Record<string, unknown>> {
        actor = holdingActor(actor, "finance.approve");
        const input = groupReportDecisionSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        const op = `finance.group-edition.${action}`;
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts;
            const repository = ports.repository;
            const { source, get } = createFinanceGroupEditionsApplication(ports).internal;
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const r = await get(actor, id);
            if (r.status !== "draft" || r.version !== input.expectedVersion)
                throw new BusinessRuleError(409, "GROUP_EDITION_CHANGED", "Refresh the draft before deciding.");
            if (samePerson(actor, r.prepared_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "Another Holding Finance approver must review the group edition.");
            const definition = groupReportEditionSchema.parse(JSON.parse(String(r.content_json))), e = action === "approve" ? await source(actor, definition) : null;
            if (e && e.hash !== r.source_hash)
                throw new BusinessRuleError(409, "GROUP_EDITION_SOURCE_CHANGED", "Period, ledger, mapping, rate or adjustment evidence changed. Reject and prepare again.");
            const steps = [];
            if (action === "approve" && r.parent_id) {
                const p = await get(actor, String(r.parent_id));
                if (p.status !== "approved" || p.version !== definition.parentVersion)
                    throw new BusinessRuleError(409, "GROUP_EDITION_PARENT_CHANGED", "The earlier edition changed before amendment approval.");
                steps.push(repository.stageDecideGroupReportEditionEffect(ports.clock.now(), p.id));
            }
            steps.push(repository.guard14(key, "group-edition-version", id, actor.holdingId, input.expectedVersion), repository.stageDecideGroupReportEditionEffect2(action === "approve" ? "approved" : "rejected", actor.userId, ports.clock.now(), id));
            return ports.commands.commit(actor, key, op, input, id, input.expectedVersion + 1, {
                id,
                status: action === "approve" ? "approved" : "rejected",
                version: input.expectedVersion + 1,
            }, steps);
        });
    }
    async function groupReportEditionExport(actor: WorkContext, id: string) {
        actor = holdingActor(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const r = await get(actor, id);
        if (!["approved", "superseded"].includes(String(r.status)))
            throw new BusinessRuleError(409, "GROUP_EDITION_NOT_APPROVED", "Approve this group reporting edition before exporting.");
        if ((await ports.sha256(String(r.export_content))) !== r.export_sha256)
            throw new BusinessRuleError(409, "GROUP_EXPORT_EVIDENCE_CHANGED", "Retained group export bytes differ from their checksum.");
        return {
            content: String(r.export_content),
            sha256: String(r.export_sha256),
            filename: `group-${r.start_date}-${r.end_date}-${r.edition}.csv`,
        };
    }
    async function groupEditionSourceJournal(actor: WorkContext, id: string, journalId: string) {
        actor = holdingActor(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const edition = await get(actor, id);
        const proof = JSON.parse(String(edition.source_json)) as {
            entities: {
                registration: Row;
                lines: Row[];
            }[];
        };
        const entity = proof.entities.find((e) => e.lines.some((l) => l.journal_id === journalId));
        if (!entity)
            throw new BusinessRuleError(404, "GROUP_SOURCE_JOURNAL_NOT_FOUND", "The source journal is not included in this retained group edition.");
        return {
            edition,
            entity: entity.registration,
            lines: entity.lines.filter((l) => l.journal_id === journalId),
        };
    }
    async function groupReportEditionWorkspace(actor: WorkContext) {
        actor = holdingActor(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        return (await repository.readGroupReportEditionWorkspaceEffect(actor.holdingId, actor.companyId)).results;
    }
    async function groupExpansionWorkspace(actor: WorkContext) {
        actor = holdingActor(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const [editions, ownership, pairs, charts, periods, accounts, documents, calculations,] = await Promise.all([
            groupReportEditionWorkspace(actor),
            ports.groupOwnershipWorkspace(actor),
            ports.intercompanyPairWorkspace(actor),
            repository.readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations(actor.holdingId),
            repository.readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations2(actor.holdingId),
            repository.readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations3(actor.holdingId),
            repository.readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations4(actor.holdingId),
            repository.readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations5(actor.holdingId, actor.companyId),
        ]);
        return {
            editions,
            ...ownership,
            pairs,
            charts: charts.results,
            periods: periods.results,
            accounts: accounts.results,
            documents: documents.results,
            calculations: calculations.results,
        };
    }
    async function groupRecordNames(row: Row, companyId: string) {
        const fields = ["created_by", "prepared_by", "reviewed_by", "approved_by"], ids = [...new Set(fields.map(field => row[field]).filter((id): id is string => typeof id === "string"))];
        if (!ids.length)
            return row;
        const people = (await repository.readGroupRecordNamesPeople(companyId, ids)).results;
        return { ...row, ...Object.fromEntries(fields.filter(field => row[field]).map(field => [`${field}_name`, people.find(person => person.user_id === row[field])?.legal_name ?? "Unlinked Finance account"])) };
    }
    async function groupExpansionRecord(actor: WorkContext, kind: "finance_group_edition" | "finance_group_ownership" | "finance_group_acquisition" | "finance_group_pair", id: string) {
        actor = holdingActor(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const table = {
            finance_group_edition: "finance_group_report_editions",
            finance_group_ownership: "finance_ownership_interests",
            finance_group_acquisition: "finance_acquisitions",
            finance_group_pair: "finance_intercompany_document_pairs",
        }[kind];
        const record = await repository.readGroupExpansionRecordRecord(actor.holdingId, actor.companyId, id, table);
        if (!record)
            throw new BusinessRuleError(404, "GROUP_EVIDENCE_NOT_FOUND", "Group evidence not found in this Company.");
        const workspace = await groupExpansionWorkspace(actor), field = {
            finance_group_edition: "editions",
            finance_group_ownership: "ownership",
            finance_group_acquisition: "acquisitions",
            finance_group_pair: "pairs",
        }[kind] as "editions" | "ownership" | "acquisitions" | "pairs";
        const row = workspace[field].find((row) => row.id === id) ?? record;
        return {
            row: await groupRecordNames(row, actor.companyId),
            groupData: {
                ...workspace,
                editions: [] as Row[],
                ownership: [] as Row[],
                acquisitions: [] as Row[],
                pairs: [] as Row[],
                [field]: [row],
            },
        };
    }
    async function groupEditionAccountSources(actor: WorkContext, id: string, accountCode: string, page: number) {
        actor = holdingActor(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const edition = await get(actor, id);
        const definition = JSON.parse(String(edition.content_json)) as Input;
        const result = JSON.parse(String(edition.result_json)) as {
            balanceSheet: {
                code: string;
                name: string;
                amount: string;
            }[];
            profitAndLoss: {
                code: string;
                name: string;
                amount: string;
            }[];
        };
        const account = [...result.balanceSheet, ...result.profitAndLoss].find((row) => row.code === accountCode);
        if (!account)
            throw new BusinessRuleError(404, "GROUP_ACCOUNT_NOT_FOUND", "Group account not found in this retained edition.");
        const accountIds = new Set(definition.mappings
            .filter((mapping) => mapping.group.code === accountCode)
            .map((mapping) => mapping.accountId));
        const proof = JSON.parse(String(edition.source_json)) as {
            entities: {
                definition: Input['entities'][number];
                registration: Row;
                lines: Row[];
            }[];
            adjustments: unknown[];
            eliminations: unknown[];
            acquisitions: unknown[];
        };
        const interval = result.profitAndLoss.some(row => row.code === accountCode);
        const sources = proof.entities.filter(entity => entity.definition.method === 'full').flatMap((entity) => entity.lines
            .filter((line) => {
            const included = accountIds.has(String(line.account_id)) || accountCode === '_cta' || accountCode === '_unclosed_profit' && profitCategory(String(line.category));
            return included && (!interval || String(line.entry_date) >= definition.startDate && !ports.capturedFiscalClose(line));
        })
            .map<Row>((line) => ({
            ...line,
            name: `${entity.registration.reporting_name} · ${line.code} · ${line.name}`,
            currency: line.currency ?? entity.registration.base_currency,
        })));
        const rows = sources.slice((page - 1) * 30, page * 30);
        return {
            row: {
                ...await groupRecordNames(edition, actor.companyId),
                name: `${account.code} · ${account.name}`,
                reported_total: `${edition.currency} ${account.amount}`,
                description: "Retained original journal amounts. The edition records currency translation and group adjustments separately.",
            },
            rows,
            page,
            hasNext: page * 30 < sources.length,
            sourceArgs: { editionId: id },
            calculationEvidence: { rates: definition.rates, mappings: definition.mappings.filter(mapping => accountIds.has(mapping.accountId)), adjustments: proof.adjustments, eliminations: proof.eliminations, acquisitions: proof.acquisitions },
        };
    }
    return { prepareGroupReportEdition, decideGroupReportEdition, groupReportEditionExport, groupEditionSourceJournal, groupReportEditionWorkspace, groupExpansionWorkspace, groupExpansionRecord, groupEditionAccountSources, internal: { holdingActor, convert, source, get, prepareGroupReportEdition, decideGroupReportEdition, groupReportEditionExport, groupEditionSourceJournal, groupReportEditionWorkspace, groupExpansionWorkspace, groupRecordNames, groupExpansionRecord, groupEditionAccountSources } };
}


