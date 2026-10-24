import { metricIds, breakdowns, filtersSchema } from "@erpx/server-runtime/public/rules/canvas-domain";
import { z } from "zod";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { RecordTarget } from "@erpx/feature-records/lib/record-drawer";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { agentEvidenceRecord } from "@erpx/server-agent/public/agent-governance";
import type { isFinanceExpansionKind } from "@erpx/server-finance/public/finance-expansion-records";
import type { financeExpansionRecord } from "@erpx/server-finance/public/finance-expansion-records";
import type { groupExpansionRecord } from "@erpx/server-finance/public/finance-group-editions";
import type { groupEditionSourceJournal } from "@erpx/server-finance/public/finance-group-editions";
import type { groupEditionAccountSources } from "@erpx/server-finance/public/finance-group-editions";
import type { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
import type { financeExtraRecord } from "@erpx/server-finance/public/finance-record-details";
import type { isFinanceExtraKind } from "@erpx/server-finance/public/finance-record-details";
import type { pmsRefundRecord } from "@erpx/server-pms/public/pms/record-details";
import type { pmsSnapshotEntry } from "@erpx/server-pms/public/pms/record-details";
import type { pmsControlSources } from "@erpx/server-pms/public/pms/record-details";
import type { queryMetric } from "@erpx/server-runtime/public/executive/metrics";
import type { isDomainRecordKind } from "@erpx/server-records/public/record-domain-details";
import type { recordDomainData } from "@erpx/server-records/public/record-domain-details";
import type {commerceOverview} from '@erpx/server-commerce/public/commerce';
import type {commercePermissions} from '@erpx/server-commerce/public/commerce';
import type {bankWorkspace} from '@erpx/server-finance/public/finance';
import type {bankStatementDetail} from '@erpx/server-finance/public/finance';
import type {listCharts} from '@erpx/server-finance/public/finance';
import type {listAccounts} from '@erpx/server-finance/public/finance';
import type {aging} from '@erpx/server-finance/public/finance';
import type {trialBalance} from '@erpx/server-finance/public/finance';
import type {statements} from '@erpx/server-finance/public/finance';
import type { marketingWorkspace } from "@erpx/server-marketing/public/marketing";
import type { payrollSummary } from "@erpx/server-people_payroll/public/people-payroll";
import type { calculationRunDetail } from "@erpx/server-finance/public/finance-engines";
import type { accrualRecordDetail } from "@erpx/server-finance/public/finance-accruals";
import type { intercompanyDetail } from "@erpx/server-finance/public/finance-intercompany";
import type { consolidationDetail } from "@erpx/server-finance/public/finance-consolidation";
import type { consolidationEliminationDetail } from "@erpx/server-finance/public/finance-consolidation";
import type { pricingWorkspace } from "@erpx/server-sales/public/sales-pricing";
import type { listInventoryReorderPolicies } from "@erpx/server-inventory/public/inventory-reorder-policies";
import type { posReportReceipts } from "@erpx/server-pos/public/pos";
import type { statementSourceAccounts } from "@erpx/server-finance/public/finance-reporting";
import type { financePlanRecord } from "@erpx/server-finance/public/finance-planning";
import type { budgetSourceRecords } from "@erpx/server-finance/public/finance-planning";
import type { financialReportSourceRecords } from "@erpx/server-finance/public/finance-planning";
import type { financeBookRecord } from "@erpx/server-finance/public/finance-books";
import { drilldownPage } from '@erpx/server-records/domain/record-details';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';

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
export interface RecordDetailsRepository {
    readCommerceMasterDetailRow(id: string, companyId: string): Promise<Row | null>;
    readCommerceMasterDetailRow2(id: string, companyId: string): Promise<Row | null>;
    readCommerceMasterDetailRow3(id: string, companyId: string, branchId: string | null, branchId2: string | null): Promise<Row | null>;
    readCommerceMasterDetailHistory(id: string): Promise<{
        results: (Row)[];
    }>;
    readRecordSupplementDataRow(id: string, companyId: string, branchId: string | null, branchId2: string | null): Promise<Row | null>;
    readRecordSupplementDataSales(companyId: string, branchId: string | null, branchId2: string | null, source_id: string | number | null, source_id2: string | number | null, source_id3: string | number | null): Promise<{
        results: (Row)[];
    }>;
    readRecordSupplementDataRow2(id: string, companyId: string, companyId2: string, branchId: string | null, branchId2: string | null): Promise<Row | null>;
    readRecordSupplementDataRows(id: string, companyId: string, branchId: string | null, branchId2: string | null, offset: number): Promise<{
        results: (Row)[];
    }>;
    readRecordSupplementDataRow3(id: string, warehouseId: string, companyId: string, branchId: string | null, branchId2: string | null): Promise<Row | null>;
    readRecordSupplementDataRows2(warehouseId: string, id: string, offset: number): Promise<{
        results: (Row)[];
    }>;
    readRecordSupplementDataAccount(id: string, companyId: string): Promise<Row | null>;
    readRecordSupplementDataPeriod(periodId: string, companyId: string): Promise<Row | null>;
    readRecordSupplementDataRows3(id: string, companyId: string, chart_id: string | number | null, value: "0" | "1", value2: string | null, value3: string | null, value4: string | null, value5: string | null, offset: number): Promise<{
        results: (Row)[];
    }>;
    readRecordSupplementDataRows4(companyId: string, id: string, currency: string, offset: number): Promise<{
        results: (Row)[];
    }>;
}
export interface RecordDetailsPorts<Effect> {
    repository: RecordDetailsRepository;
    clock: Clock;
    identifiers: IdentifierSource;

    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    reportDates(value: unknown): {
        startDate?: string;
        endDate?: string;
    };
    agentEvidenceRecord(...args: Parameters<typeof agentEvidenceRecord>): PortResult<ReturnType<typeof agentEvidenceRecord>, Effect>;
    isFinanceExpansionKind: typeof isFinanceExpansionKind;
    financeExpansionRecord(...args: Parameters<typeof financeExpansionRecord>): PortResult<ReturnType<typeof financeExpansionRecord>, Effect>;
    groupExpansionRecord(...args: Parameters<typeof groupExpansionRecord>): PortResult<ReturnType<typeof groupExpansionRecord>, Effect>;
    groupEditionSourceJournal(...args: Parameters<typeof groupEditionSourceJournal>): PortResult<ReturnType<typeof groupEditionSourceJournal>, Effect>;
    groupEditionAccountSources(...args: Parameters<typeof groupEditionAccountSources>): PortResult<ReturnType<typeof groupEditionAccountSources>, Effect>;
    financeCompanyTools(...args: Parameters<typeof financeCompanyTools>): PortResult<ReturnType<typeof financeCompanyTools>, Effect>;
    financeExtraRecord(...args: Parameters<typeof financeExtraRecord>): PortResult<ReturnType<typeof financeExtraRecord>, Effect>;
    isFinanceExtraKind: typeof isFinanceExtraKind;
    pmsRefundRecord(...args: Parameters<typeof pmsRefundRecord>): PortResult<ReturnType<typeof pmsRefundRecord>, Effect>;
    pmsSnapshotEntry(...args: Parameters<typeof pmsSnapshotEntry>): PortResult<ReturnType<typeof pmsSnapshotEntry>, Effect>;
    pmsControlSources(...args: Parameters<typeof pmsControlSources>): PortResult<ReturnType<typeof pmsControlSources>, Effect>;
    queryMetric(...args: Tail<Parameters<typeof queryMetric>>): PortResult<ReturnType<typeof queryMetric>, Effect>;
    isDomainRecordKind: typeof isDomainRecordKind;
    recordDomainData(...args: Parameters<typeof recordDomainData>): PortResult<ReturnType<typeof recordDomainData>, Effect>;
    commerceOverview(...args: Parameters<typeof commerceOverview>): PortResult<ReturnType<typeof commerceOverview>, Effect>;
    commercePermissions(...args: Parameters<typeof commercePermissions>): PortResult<ReturnType<typeof commercePermissions>, Effect>;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
    bankWorkspace(...args: Parameters<typeof bankWorkspace>): PortResult<ReturnType<typeof bankWorkspace>, Effect>;
    bankStatementDetail(...args: Parameters<typeof bankStatementDetail>): PortResult<ReturnType<typeof bankStatementDetail>, Effect>;
    listCharts(...args: Parameters<typeof listCharts>): PortResult<ReturnType<typeof listCharts>, Effect>;
    listAccounts(...args: Parameters<typeof listAccounts>): PortResult<ReturnType<typeof listAccounts>, Effect>;
    aging(...args: Parameters<typeof aging>): PortResult<ReturnType<typeof aging>, Effect>;
    trialBalance(...args: Parameters<typeof trialBalance>): PortResult<ReturnType<typeof trialBalance>, Effect>;
    statements(...args: Parameters<typeof statements>): PortResult<ReturnType<typeof statements>, Effect>;
    marketingWorkspace(...args: Parameters<typeof marketingWorkspace>): PortResult<ReturnType<typeof marketingWorkspace>, Effect>;
    payrollSummary(...args: Parameters<typeof payrollSummary>): PortResult<ReturnType<typeof payrollSummary>, Effect>;
    calculationRunDetail(...args: Parameters<typeof calculationRunDetail>): PortResult<ReturnType<typeof calculationRunDetail>, Effect>;
    accrualRecordDetail(...args: Parameters<typeof accrualRecordDetail>): PortResult<ReturnType<typeof accrualRecordDetail>, Effect>;
    intercompanyDetail(...args: Parameters<typeof intercompanyDetail>): PortResult<ReturnType<typeof intercompanyDetail>, Effect>;
    consolidationDetail(...args: Parameters<typeof consolidationDetail>): PortResult<ReturnType<typeof consolidationDetail>, Effect>;
    consolidationEliminationDetail(...args: Parameters<typeof consolidationEliminationDetail>): PortResult<ReturnType<typeof consolidationEliminationDetail>, Effect>;
    pricingWorkspace(...args: Parameters<typeof pricingWorkspace>): PortResult<ReturnType<typeof pricingWorkspace>, Effect>;
    listInventoryReorderPolicies(...args: Parameters<typeof listInventoryReorderPolicies>): PortResult<ReturnType<typeof listInventoryReorderPolicies>, Effect>;
    posReportReceipts(...args: Parameters<typeof posReportReceipts>): PortResult<ReturnType<typeof posReportReceipts>, Effect>;
    statementSourceAccounts(...args: Parameters<typeof statementSourceAccounts>): PortResult<ReturnType<typeof statementSourceAccounts>, Effect>;
    financePlanRecord(...args: Parameters<typeof financePlanRecord>): PortResult<ReturnType<typeof financePlanRecord>, Effect>;
    budgetSourceRecords(...args: Parameters<typeof budgetSourceRecords>): PortResult<ReturnType<typeof budgetSourceRecords>, Effect>;
    financialReportSourceRecords(...args: Parameters<typeof financialReportSourceRecords>): PortResult<ReturnType<typeof financialReportSourceRecords>, Effect>;
    financeBookRecord(...args: Parameters<typeof financeBookRecord>): PortResult<ReturnType<typeof financeBookRecord>, Effect>;
}
export function createRecordDetailsApplication<Effect>(ports: RecordDetailsPorts<Effect>) {
    const r = ports.repository;
    async function commerceMasterDetail(actor: WorkContext, kind: "product" | "party" | "warehouse", id: string) {
        ports.requirePermission(actor, "commerce.read");
        await ports.prepare(actor.holdingId);
        const row = kind === "product"
            ? await r.readCommerceMasterDetailRow(id, actor.companyId)
            : kind === "party"
                ? await r.readCommerceMasterDetailRow2(id, actor.companyId)
                : await r.readCommerceMasterDetailRow3(id, actor.companyId, actor.branchId, actor.branchId);
        if (!row)
            throw new BusinessRuleError(404, "RECORD_NOT_FOUND", "Record not found in this workspace.");
        const permissions = ports.commercePermissions(actor), canEdit = kind === "party" ? row.kind === "supplier" ? permissions.purchase : permissions.sales : permissions.stock;
        const options = canEdit ? await ports.commerceOverview(actor) : null;
        const history = (await r.readCommerceMasterDetailHistory(id)).results;
        return { kind, row, options, history, canEdit };
    }
    async function recordSupplementData(actor: WorkContext, target: RecordTarget, args: Record<string, string>) {
        await ports.prepare(actor.holdingId);
        const page = drilldownPage(args.detailPage);
        const offset = (page - 1) * 30;
        if (target.kind === 'executive_sources' || target.kind === 'executive_source') {
            const metric = z.enum(metricIds).parse(target.kind === 'executive_sources' ? target.id : args.metric), breakdown = z.enum(breakdowns).default('default').parse(args.breakdown), filters = filtersSchema.parse({ from: args.from, to: args.to, currency: args.currency });
            const result = await ports.queryMetric(actor, metric, filters, breakdown, { label: z.string().min(1).max(2000).parse(args.label), unit: args.unit, page, recordId: target.kind === 'executive_source' ? target.id : undefined });
            if (target.kind === 'executive_source') {
                const source = result.rows[0];
                if (!source)
                    throw new BusinessRuleError(404, 'SOURCE_NOT_FOUND', 'Source record not found in this report group.');
                return { kind: target.kind, row: { ...source.facts, id: source.id, name: source.facts.name ?? source.facts.title ?? source.facts.number ?? source.id }, related: source.owner_kind && source.owner_id ? [{ kind: source.owner_kind, id: source.owner_id, args: source.owner_args ?? undefined }] : [] };
            }
            return { kind: target.kind, ...result };
        }
        if (ports.isFinanceExtraKind(target.kind))
            return ports.financeExtraRecord(actor, target.kind, target.id, args, page);
        if (ports.isDomainRecordKind(target.kind))
            return await ports.recordDomainData(actor, target.kind, target.id);
        if (target.kind === 'pos_report')
            return { kind: target.kind, ...await ports.posReportReceipts(actor, { businessDate: z.iso.date().parse(target.id), registerId: z.string().min(1).max(180).parse(args.registerId), shiftId: z.string().min(1).max(180).parse(args.shiftId), currency: z.string().length(3).parse(args.currency) }, page) };
        if (target.kind === 'statement_sources')
            return { kind: target.kind, ...await ports.statementSourceAccounts(actor, target.id, args, page) };
        if (target.kind === 'pms_booking_refund' || target.kind === 'pms_counter_refund')
            return { kind: target.kind, ...await ports.pmsRefundRecord(actor, target.kind === 'pms_booking_refund' ? 'booking' : 'counter', target.id) };
        if (target.kind === 'pms_snapshot_entry')
            return { kind: target.kind, ...await ports.pmsSnapshotEntry(actor, target.id, z.string().min(1).max(180).parse(args.documentId)) };
        if (target.kind === 'pms_control_sources')
            return { kind: target.kind, ...await ports.pmsControlSources(actor, target.id, args.control, page) };
        if (target.kind === 'consolidation_elimination')
            return { kind: target.kind, ...await ports.consolidationEliminationDetail(actor, target.id) };
        if (target.kind === 'agent_invocation' || target.kind === 'agent_session')
            return { kind: target.kind, ...await ports.agentEvidenceRecord(actor, target.kind, target.id) };
        if (ports.isFinanceExpansionKind(target.kind))
            return ports.financeExpansionRecord(actor, target.kind, target.id);
        if (target.kind === 'finance_group_edition' || target.kind === 'finance_group_ownership' || target.kind === 'finance_group_acquisition' || target.kind === 'finance_group_pair')
            return { kind: target.kind, ...await ports.groupExpansionRecord(actor, target.kind, target.id), enabled: (await ports.financeCompanyTools(actor)).some(tool => tool.tool === 'group_reporting' && tool.enabled === 1) };
        if (target.kind === 'finance_group_sources')
            return { kind: target.kind, ...await ports.groupEditionAccountSources(actor, target.id, z.string().min(1).max(200).parse(args.accountCode), page) };
        if (target.kind === 'finance_group_source') {
            const source = await ports.groupEditionSourceJournal(actor, z.string().min(1).max(180).parse(args.editionId), target.id);
            return { kind: target.kind, row: { ...source.lines[0], id: target.id, name: `${source.entity.reporting_name} · ${source.lines[0]?.entry_date} · Retained journal` }, retainedLines: source.lines };
        }
        if (target.kind === 'finance_plan')
            return { kind: target.kind, ...await ports.financePlanRecord(actor, target.id) };
        if (target.kind === 'finance_book')
            return { kind: target.kind, ...await ports.financeBookRecord(actor, target.id) };
        if (target.kind === 'finance_budget_sources')
            return { kind: target.kind, ...await ports.budgetSourceRecords(actor, target.id, z.coerce.number().int().min(0).max(9999).parse(args.line), page) };
        if (target.kind === 'finance_report_snapshot')
            return { kind: target.kind, ...await ports.financialReportSourceRecords(actor, target.id, {}, page) };
        if (target.kind === 'finance_report_row')
            return { kind: target.kind, ...await ports.financialReportSourceRecords(actor, target.id, args, page) };
        if (target.kind === 'inventory_movement') {
            ports.requirePermission(actor, 'commerce.read');
            ports.requirePermission(actor, 'inventory.read');
            const row = await r.readRecordSupplementDataRow(target.id, actor.companyId, actor.branchId, actor.branchId);
            if (!row)
                throw new BusinessRuleError(404, 'MOVEMENT_NOT_FOUND', 'Inventory movement not found in this workspace.');
            const related: RecordTarget[] = [{ kind: 'product', id: String(row.product_id) }, { kind: 'stock_position', id: String(row.product_id), args: { warehouseId: String(row.warehouse_id) } }];
            const sourceKinds: Record<string, RecordTarget['kind']> = { fulfillment: 'order', customer_return: 'order', supplier_return: 'order', stock_count: 'stock_count', transfer: 'stock_transfer', maintenance: 'maintenance', field_service: 'field_service', manufacturing: 'manufacturing_order', landed_cost: 'landed_cost' };
            const sourceType = String(row.source_type), kind = sourceKinds[sourceType] ?? (sourceType.startsWith('transit') ? 'stock_transit' : sourceType.startsWith('manufacturing') ? 'manufacturing_order' : null);
            if (kind)
                related.push({ kind, id: String(row.source_id) });
            if (sourceType.startsWith('pos_') && hasPermission(actor, 'pos.read') && !actor.departmentId) {
                const sales = (await r.readRecordSupplementDataSales(actor.companyId, actor.branchId, actor.branchId, row.source_id, row.source_id, row.source_id)).results;
                related.push(...sales.map(sale => ({ kind: 'pos_sale' as const, id: String(sale.id) })));
            }
            return { kind: target.kind, row, related };
        }
        if (target.kind === "product" || target.kind === "party" || target.kind === "warehouse")
            return commerceMasterDetail(actor, target.kind, target.id);
        if (target.kind === "stock_position" || target.kind === "inventory_lot") {
            ports.requirePermission(actor, "commerce.read");
            ports.requirePermission(actor, "inventory.read");
            if (target.kind === "inventory_lot") {
                const row = await r.readRecordSupplementDataRow2(target.id, actor.companyId, actor.companyId, actor.branchId, actor.branchId);
                if (!row)
                    throw new BusinessRuleError(404, "RECORD_NOT_FOUND", "Lot not found in this workspace.");
                const rows = (await r.readRecordSupplementDataRows(target.id, actor.companyId, actor.branchId, actor.branchId, offset)).results;
                return { kind: target.kind, row, rows, page, hasNext: rows.length > 30 };
            }
            const warehouseId = z.string().min(1).max(180).parse(args.warehouseId);
            const row = await r.readRecordSupplementDataRow3(target.id, warehouseId, actor.companyId, actor.branchId, actor.branchId);
            if (!row)
                throw new BusinessRuleError(404, "RECORD_NOT_FOUND", "Stock position not found in this workspace.");
            const rows = (await r.readRecordSupplementDataRows2(warehouseId, target.id, offset)).results;
            return { kind: target.kind, row, rows, page, hasNext: rows.length > 30 };
        }
        if (target.kind === 'bank_transaction') {
            const data = await ports.bankWorkspace(actor, { match: target.id });
            return { kind: target.kind, row: data.selectedTransaction!, data };
        }
        if (target.kind === 'bank_statement')
            return { kind: target.kind, ...await ports.bankStatementDetail(actor, target.id, page) };
        if (target.kind === 'marketing_campaign_sources') {
            const data = await ports.marketingWorkspace(actor), source = z.string().max(180).parse(args.source), all = data.candidates.filter(row => {
                const attribution = JSON.parse(String(row.attribution_json ?? '{}'));
                const normalized = (value: unknown) => typeof value === 'string' && value.trim() ? value.trim() : 'Unspecified';
                return normalized(attribution.campaign) === target.id;
            });
            const matching = all.filter(row => { const a = JSON.parse(String(row.attribution_json ?? '{}')); return (String(a.source ?? '').trim() || String(a.utmSource ?? '').trim() || 'Unspecified') === source; });
            return { kind: target.kind, row: { name: target.id, source, count: matching.length }, rows: matching.slice(offset, offset + 30), page, hasNext: matching.length > offset + 30 };
        }
        if (target.kind === "finance_report") {
            const name = z.enum(['assets', 'liabilities', 'equity', 'revenue', 'expenses']).parse(target.id), chartId = z.string().min(1).parse(args.chartId), dates = ports.reportDates({ startDate: args.startDate || undefined, endDate: args.endDate || undefined });
            const report = await ports.statements(actor, undefined, chartId, dates), income = ['revenue', 'expenses'].includes(name), sourceArgs = { chartId, ...(income && dates.startDate ? { startDate: dates.startDate } : {}), ...(dates.endDate ? { endDate: dates.endDate } : {}), ...(income ? { excludeYearClose: '1' } : {}) };
            const categories = name === 'assets' ? ['asset'] : name === 'liabilities' ? ['liability'] : name === 'equity' ? ['equity', 'revenue', 'other_income', 'expense', 'other_expense'] : name === 'revenue' ? ['revenue'] : ['expense'];
            const all = (await ports.trialBalance(actor, undefined, chartId, income ? dates : { endDate: dates.endDate }, { excludeYearClose: income })).filter(row => categories.includes(String(row.category)));
            const total = name === 'assets' ? report.balanceSheet.assets : name === 'liabilities' ? report.balanceSheet.liabilities : name === 'equity' ? report.balanceSheet.equity : name === 'revenue' ? report.profitAndLoss.revenue : report.profitAndLoss.expenses;
            return { kind: target.kind, row: { name, reported_total: total }, rows: all.slice(offset, offset + 30), sourceArgs, page, hasNext: all.length > offset + 30 };
        }
        if (target.kind === "finance_account" || target.kind === "trial_balance") {
            if (args.statementId)
                await ports.statementSourceAccounts(actor, args.statementId, { period: 'current' }, 1);
            const charts = await ports.listCharts(actor);
            let account = await r.readRecordSupplementDataAccount(target.id, actor.companyId);
            if (!account || !charts.some(chart => chart.id === account!.chart_id))
                throw new BusinessRuleError(404, "ACCOUNT_NOT_FOUND", "Account not found.");
            // Apply the same report validation and posted/reversed predicates as the owner.
            const dates = ports.reportDates({ startDate: args.startDate || undefined, endDate: args.endDate || undefined });
            if (args.chartId && args.chartId !== account.chart_id)
                throw new BusinessRuleError(404, "ACCOUNT_NOT_FOUND", "Account is outside the selected ledger.");
            await ports.listAccounts(actor, String(account.chart_id));
            if (args.periodId) {
                const period = await r.readRecordSupplementDataPeriod(args.periodId, actor.companyId);
                if (!period)
                    throw new BusinessRuleError(404, "PERIOD_NOT_FOUND", "Period not found.");
                dates.startDate = String(period.start_date);
                dates.endDate = String(period.end_date);
            }
            const totals = await ports.trialBalance(actor, undefined, String(account.chart_id), dates, { excludeYearClose: args.excludeYearClose === '1' });
            account = { ...account, ...totals.find(row => row.id === target.id) };
            const rows = (await r.readRecordSupplementDataRows3(target.id, actor.companyId, account.chart_id, args.excludeYearClose === '1' ? '1' : '0', dates.startDate ?? null, dates.startDate ?? null, dates.endDate ?? null, dates.endDate ?? null, offset)).results;
            return { kind: target.kind, row: account, rows, page, hasNext: rows.length > 30 };
        }
        if (target.kind === "aging_bucket") {
            const kind = z.enum(['payable', 'receivable']).parse(args.kind), asOf = z.iso.date().parse(args.asOf), currency = z.string().length(3).parse(args.currency), bucket = z.enum(['current', '1-30', '31-60', '61-90', '90+']).parse(target.id);
            const all = (await ports.aging(actor, kind, asOf, args.chartId)).filter(row => row.bucket === bucket && (row as unknown as Row).currency === currency);
            return { kind: target.kind, row: { name: `${kind} · ${bucket} · ${currency}`, asOf, currency, count: all.length, outstanding: all.reduce((sum, row) => sum + Number(row.outstanding_minor), 0) }, rows: all.slice(offset, offset + 30), page, hasNext: all.length > offset + 30 };
        }
        if (target.kind === "payroll_month") {
            const currency = z.string().length(3).parse(args.currency), summary = await ports.payrollSummary(actor, target.id);
            const row = summary.find(row => row.currency === currency);
            if (!row)
                throw new BusinessRuleError(404, "PAYROLL_MONTH_NOT_FOUND", "No posted payroll in this month and currency.");
            const rows = (await r.readRecordSupplementDataRows4(actor.companyId, target.id, currency, offset)).results;
            return { kind: target.kind, row: { ...row, name: `Posted payroll · ${target.id}` }, rows, page, hasNext: rows.length > 30 };
        }
        if (target.kind === "accrual_schedule" || target.kind === "accrual_run")
            return { kind: target.kind, ...await ports.accrualRecordDetail(actor, target.kind === "accrual_schedule" ? "schedule" : "run", target.id) };
        if (target.kind === "intercompany")
            return { kind: target.kind, ...await ports.intercompanyDetail(actor, target.id) };
        if (target.kind === "consolidation")
            return { kind: target.kind, ...await ports.consolidationDetail(actor, target.id) };
        if (target.kind === "price_list") {
            const data = await ports.pricingWorkspace(actor), row = data.lists.find(row => row.id === target.id);
            if (!row)
                throw new BusinessRuleError(404, "PRICE_LIST_NOT_FOUND", "Price list not found.");
            return { kind: target.kind, row, data: { ...data, lists: [row], items: data.items.filter(item => item.price_list_id === target.id) } };
        }
        if (target.kind === "reorder_policy") {
            const row = (await ports.listInventoryReorderPolicies(actor)).find(row => row.id === target.id);
            if (!row)
                throw new BusinessRuleError(404, "REORDER_POLICY_NOT_FOUND", "Reorder policy not found.");
            const options = await ports.commerceOverview(actor);
            return { kind: target.kind, row, options };
        }
        if (target.kind === "calculation_run") {
            const run = await ports.calculationRunDetail(actor, target.id);
            return { kind: target.kind, row: run, rows: run.events, page: 1, hasNext: false };
        }
        throw new BusinessRuleError(404, "RECORD_KIND_UNAVAILABLE", "This record does not have a detail reader.");
    }
    return { commerceMasterDetail, drilldownPage, recordSupplementData };
}
