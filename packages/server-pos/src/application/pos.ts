import { posPermissions } from "@erpx/server-pos/domain/pos-permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { str, scopeRow as domainScopeRow, type PosRow } from "@erpx/server-pos/domain/pos-model";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { businessDate as evaluateBusinessDate, recipeQuantities } from "@erpx/server-pos/domain/pos";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { ListPage } from "@erpx/server-work_projection/public/list-page";
import type { PosReceiptSnapshot } from "@erpx/server-pos/public/pos-daily";
export type PosExecutiveEvidenceMetric = "pos-sales" | "net-pos-sales" | "pos-payment-mix";
export type PosRows<Row> = {
    results: Row[];
};
export type PosReadEffect = {
    run(): Promise<PosRows<Record<string, unknown>>>;
};
export type PosReadAttachmentBinding<Effect> = {
    guards: Effect[];
    steps: Effect[];
};
export type PosReadUnit<Effect extends PosReadEffect> = {
    hotelLinks(companyId: string, branchId: string | null): Promise<PosRow[]>;
    readPosRegisters1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readPosSales1(facts: {
        saleId: string;
    }): Promise<PosRow | null>;
    readPosSettings1(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readCommerceProducts1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow | null>;
    readPosRegisters2(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readCommerceSettings1(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readCommerceSettings2(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readPosRegisterCatalog1(facts: {
        branch: string | null;
        branch2: string | null;
        company: string;
        fact3: boolean;
        branch3: string | null;
        branch4: string | null;
    }): Promise<PosRow[]>;
    readCommerceWarehouses1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readPosPayments1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readPosTables1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readPosSales2(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
        fact3: string;
        fact4: string;
        fact5: 0 | 1;
    }): Promise<PosRow[]>;
    readPosLines1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readPosRefunds1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
        fact3: string | number | boolean | null | undefined;
        fact4: string | number | boolean | null | undefined;
    }): Promise<PosRow[]>;
    readPosCashMovements1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readInventoryActiveReservations1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readFinanceAccounts1(facts: {
        company: string;
    }): Promise<PosRow[]>;
    readPosLines2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readPosPayments2(facts: {
        id: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readFinanceBankTransactions1(facts: {
        company: string;
        fact1: unknown;
        currency: string | number | boolean | null;
        amount_minor: string | number | boolean | null;
        reference: string | number | boolean | null;
        business_date: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readPmsSources1(facts: {
        id: string | number | boolean | null;
        fact1: string;
        company: string;
        branch_id: string | number | boolean | null;
    }): Promise<PosRow | null>;
    readPosConnections1(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readPmsProperties1(facts: {
        company: string;
        branch_id: string | number | boolean | null;
        currency: string | number | boolean | null;
        propertyId: string | number | boolean | null;
    }): Promise<PosRow[]>;
    readPosRefundPayments1(facts: {
        company: string;
    }): Promise<PosRow[]>;
    readPosLines3(facts: {
        company: string;
        company2: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readPosRefundPayments2(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readPosSaleDocuments1(facts: {
        id: string | number | boolean | null;
        company: string;
    }): Promise<PosRow | null>;
    readOrganizationNodes1(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readFinanceBankTransactions2(facts: {
        company: string;
    }): Promise<PosRow[]>;
    readOrganizationNodes2(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readPosDayCloses1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
    }): Promise<PosRow[]>;
    readFinanceAccounts2(facts: {
        company: string;
    }): Promise<PosRow[]>;
    readFinanceCharts1(facts: {
        company: string;
    }): Promise<PosRow[]>;
    readPosConnections2(facts: {
        company: string;
    }): Promise<PosRow | null>;
    readPosSettlements1(facts: {
        company: string;
    }): Promise<PosRow[]>;
    readPosSales3(facts: {
        id: string;
        company: string;
        branch: string | null;
        branch2: string | null;
        currency: string;
        from: string;
        to: string;
    }): Promise<Record<string, unknown> | null>;
    readPosSales4(facts: {
        id: string;
        company: string;
        branch: string | null;
        branch2: string | null;
        currency: string;
        from: string;
        to: string;
    }): Promise<Record<string, unknown> | null>;
    readPosRefunds2(facts: {
        id: string;
        company: string;
        branch: string | null;
        branch2: string | null;
        currency: string;
        from: string;
        to: string;
    }): Promise<Record<string, unknown> | null>;
    readPosRefundPayments3(facts: {
        id: string;
        company: string;
        branch: string | null;
        branch2: string | null;
        currency: string;
        from: string;
        to: string;
    }): Promise<Record<string, unknown> | null>;
    readPosRefunds3(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
        userId: string;
    }): Promise<PosRow[]>;
    readPosShifts1(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
        userId: string;
    }): Promise<PosRow[]>;
    readPosCashMovements2(facts: {
        company: string;
        branch: string | null;
        branch2: string | null;
        userId: string;
    }): Promise<PosRow[]>;
    readPosJobs1(facts: {
        company: string;
    }): Promise<PosRow[]>;
    readPosRegisters3(facts: {
        registerId: string;
    }): Promise<PosRow | null>;
    readPosLines4(facts: {
        companyId: string;
        companyId2: string;
        branchId: string | null;
        branchId2: string | null;
        businessDate: string;
        registerId: string;
        shiftId: string;
        currency: string;
        fact8: number;
    }): Promise<PosRow[]>;
    posCustomers(arg0: WorkContext): Promise<PosRow[]>;
    salesPage(facts: {
        company: string;
        branch: string | null;
        historyQuery: string;
        cursor?: string;
    }): Promise<ListPage<PosRow & {
        id: string;
        page_key: string;
    }>>;
    batch(effects: Effect[]): Promise<unknown>;
};
export function createPosRead<Effect extends PosReadEffect>(ports: {
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    hash(value: string): string;
    environment(key: "NODE_ENV" | "ERPX_POS_ROOM_CHARGE_PILOT_ENABLED" | "POS_STRIPE_LIVE_ENABLED"): string | undefined;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    forActor(actor: WorkContext): Promise<PosReadUnit<Effect>>;
}) {
    const businessDate = (timezone = "Asia/Bangkok", date = new Date(ports.clock.now())) => evaluateBusinessDate(timezone, date);
    const scopeRow = (actor: WorkContext, row: PosRow | null) => domainScopeRow(actor, row, ports.error);
    async function posWorkspace(actor: WorkContext, saleId?: string, historyQuery = '', section?: string, cursor?: string, frontMode = false, financeMode = false) {
        ports.require(actor, financeMode ? 'finance.read' : 'pos.read');
        if (financeMode && (frontMode || !['settings','reports'].includes(section ?? ''))) throw ports.error(403,'FINANCE_SOURCE_SELECTION_REQUIRED','Choose accounting settings or a reconciliation report.');
        if (actor.departmentId)
            throw ports.error(403, 'POS_BRANCH_REQUIRED', 'Select a Company or Branch to read POS reports.');
        const db = await ports.forActor(actor), company = actor.companyId, branch = actor.branchId;
        const registers = await db.readPosRegisters1({ company: company, branch: branch, branch2: branch });
        const sale = saleId ? scopeRow(actor, await db.readPosSales1({ saleId: saleId })) : null;
        const wants = (...tabs: string[]) => !section || tabs.includes(section);
        const settings = await db.readPosSettings1({ company: company });
        const frontProductCount = frontMode ? await db.readCommerceProducts1({ company: company, branch: branch, branch2: branch }) : null;
        const frontHotels = frontMode ? await db.readPosRegisters2({ company: company, branch: branch, branch2: branch }) : [];
        const frontBlockedRegisters = frontHotels.filter(row => row.hotel_state && (row.hotel_state !== 'open' || row.business_date !== businessDate()));
        const accounting = !frontMode && (hasPermission(actor,'pos.accounting.prepare') || hasPermission(actor,'pos.accounting.approve'));
        const report = !frontMode && (hasPermission(actor, 'pos.reports') || financeMode), configure = !frontMode && hasPermission(actor, 'pos.configure') && (wants('settings') || !settings), connect = !frontMode && hasPermission(actor, 'pos.payments.configure') && wants('settings');
        const salesPage = !financeMode && section === 'history' ? await db.salesPage({ company, branch, historyQuery, cursor }) : null;
        const [commerce, commerceCurrency, products, warehouses, shifts, tables, sales, kitchen, refunds, cashMovements, stock, bankAccounts] = await Promise.all([
            accounting ? db.readCommerceSettings1({ company: company }) : Promise.resolve(null),
            db.readCommerceSettings2({ company: company }),
            !financeMode && wants('sale', 'settings') ? db.readPosRegisterCatalog1({ branch: branch, branch2: branch, company: company, fact3: section === 'settings', branch3: branch, branch4: branch }) : Promise.resolve([]),
            wants('sale', 'settings') ? db.readCommerceWarehouses1({ company: company, branch: branch, branch2: branch }) : Promise.resolve([]),
            wants('sale', 'history', 'shifts') ? db.readPosPayments1({ company: company, branch: branch, branch2: branch }) : Promise.resolve([]),
            !financeMode && wants('sale', 'settings') ? db.readPosTables1({ company: company, branch: branch, branch2: branch }) : Promise.resolve([]),
            salesPage ? Promise.resolve(salesPage.items) : wants('sale') ? db.readPosSales2({ company: company, branch: branch, branch2: branch, fact3: historyQuery.slice(0, 100), fact4: `%${historyQuery.slice(0, 100)}%`, fact5: section === 'sale' ? 1 : 0 }) : Promise.resolve([]),
            wants('kitchen') && (hasPermission(actor,'pos.kitchen') || hasPermission(actor,'pos.sell')) ? db.readPosLines1({ company: company, branch: branch, branch2: branch }) : Promise.resolve([]),
            wants('history') || (wants('sale') && sale) ? db.readPosRefunds1({ company: company, branch: branch, branch2: branch, fact3: wants('history') ? null : sale?.id, fact4: wants('history') ? null : sale?.id }) : Promise.resolve([]),
            wants('shifts') ? db.readPosCashMovements1({ company: company, branch: branch, branch2: branch }) : Promise.resolve([]),
            wants('sale', 'settings') ? db.readInventoryActiveReservations1({ company: company, branch: branch, branch2: branch }) : Promise.resolve([]),
            accounting ? db.readFinanceAccounts1({ company: company }) : Promise.resolve([])
        ]);
        const lines = sale ? await db.readPosLines2({ id: sale.id }) : [];
        const payments = sale ? await db.readPosPayments2({ id: sale.id }) : [];
        const pendingTransfer = hasPermission(actor, 'finance.approve') ? payments.find(p => p.method === 'transfer' && p.state === 'pending') : null;
        const transferEvidence = pendingTransfer && sale ? await db.readFinanceBankTransactions1({ company: company, fact1: JSON.parse(str(sale, 'accounts_json')).bank, currency: sale.currency, amount_minor: pendingTransfer.amount_minor, reference: pendingTransfer.reference, business_date: sale.business_date }) : [];
        const roomCharge = sale ? await db.readPmsSources1({ id: sale.id, fact1: `pos:${sale.id}:${sale.id}`, company: company, branch_id: sale.branch_id }) : null;
        const roomChargeEnabled = ports.environment("NODE_ENV") !== 'production' || ports.environment("ERPX_POS_ROOM_CHARGE_PILOT_ENABLED") === 'true';
        const tenderConnection = await db.readPosConnections1({ company: company });
        const providerAllowed = ports.environment("NODE_ENV") !== 'production' || ports.environment("POS_STRIPE_LIVE_ENABLED") === 'true';
        const availableTenderMethods = ['cash', 'transfer', ...(providerAllowed ? ['edc'] : []), ...(providerAllowed && tenderConnection?.charges_enabled && tenderConnection.payouts_enabled && (!tenderConnection.livemode || ports.environment("POS_STRIPE_LIVE_ENABLED") === 'true') ? [...(tenderConnection.card_enabled ? ['stripe_card'] : []), ...(tenderConnection.promptpay_enabled ? ['stripe_promptpay'] : [])] : [])];
        const roomChargeStays = roomChargeEnabled && sale && ['open', 'held'].includes(str(sale, 'state')) && hasPermission(actor, 'pos.sell')
            ? await db.readPmsProperties1({ company: company, branch_id: sale.branch_id, currency: sale.currency, propertyId: registers.find(r => r.id === sale.register_id)?.property_id ?? null })
            : [];
        const jobs = !branch && (connect || hasPermission(actor, 'pos.approve')) ? await db.readPosRefundPayments1({ company: company }) : [];
        if (!connect)
            for (const job of jobs)
                delete job.payload_json;
        if (!report) {
            for (const f of kitchen)
                delete f.cost_minor;
            for (const l of lines)
                delete l.fulfillment_cost;
        }
        const reports = report && wants('reports') ? await db.readPosLines3({ company: company, company2: company, branch: branch, branch2: branch }) : [];
        const paymentMix = report && wants('reports') ? await db.readPosRefundPayments2({ company: company, branch: branch, branch2: branch }) : [];
        const frontPermissionNames = ['pos.read', 'pos.sell', 'pos.kitchen', 'pos.approve'] as const;
        const permissions = frontMode
            ? Object.fromEntries(frontPermissionNames.map((name) => [name, hasPermission(actor, name)]))
            : posPermissions(actor);
        const frontRow = (row: PosRow | null): PosRow | null => row ? Object.fromEntries(Object.entries(row).filter(([key]) => ![
            'accounts_json', 'chart_id', 'account_id', 'bank_account', 'journal_id', 'finance_payment_id', 'invoice_id', 'debit_account_id', 'credit_account_id', 'tax_account_id', 'provider_id', 'provider_customer_id', 'provider_payment_intent', 'provider_received_at', 'secret_json', 'payload_json', 'recipe_json', 'cost_minor', 'cost_price', 'unit_cost_minor', 'fulfillment_cost', 'margin_minor', 'gross_profit_minor', 'source_key', 'external_id', 'bank_transaction_id'
        ].includes(key))) as PosRow : null;
        const frontRows = (rows: PosRow[]) => frontMode ? rows.map((row) => frontRow(row) as PosRow) : rows;
        const frontSettings = settings ? frontRow({ enabled: settings.enabled, currency: settings.currency, timezone: settings.timezone }) : null;
        for (const product of products) {
            const availability: Record<string, number> = {};
            for (const register of registers) {
                if (!JSON.parse(str(product, 'register_ids_json')).includes(register.id))
                    continue;
                const parts = product.kind === 'recipe' ? recipeQuantities(JSON.parse(str(product, 'recipe_json')), 1000) : product.kind === 'stock' ? [{ productId: str(product, 'id'), quantityMilli: 1000 }] : [];
                availability[str(register, 'id')] = parts.length ? Math.max(0, Math.floor(Math.min(...parts.map(part => { const row = stock.find(s => s.product_id === part.productId && s.warehouse_id === register.warehouse_id); return row && !row.quality_held ? (Number(row.quantity_milli) - Number(row.reserved_milli)) / part.quantityMilli : 0; })) * 1000)) : 999999999;
            }
            product.availability_json = JSON.stringify(availability);
        }
        const receiptRow = sale ? await db.readPosSaleDocuments1({ id: sale.id, company: company }) : null;
        const receipt: PosReceiptSnapshot | null = receiptRow ? JSON.parse(str(receiptRow, 'snapshot_json')) : null;
        if (frontMode && receipt)
            receipt.payments = receipt.payments.map(p => { const { finance_payment_id: _finance, ...rest } = p; void _finance; return rest; });
        if (!accounting && !report && settings) delete settings.accounts_json;
        if (!accounting && !report && sale) delete sale.accounts_json;
        const hotelLinks = configure ? await db.hotelLinks(company,branch) : [];
        const frontData = { hotelLinks, currency: settings?.currency ? String(settings.currency) : commerce?.currency ? String(commerce.currency) : commerceCurrency?.currency ? String(commerceCurrency.currency) : null, salesPage: frontMode ? null : salesPage, permissions, userId: actor.userId, settings: frontMode ? frontSettings : settings, commerce: frontMode ? null : accounting ? commerce : null, availableTenderMethods, roomChargeEnabled, registers: frontRows(registers), products: frontRows(products), warehouses: frontRows(warehouses), shifts: frontRows(shifts), tables: frontRows(tables), sales: frontRows(sales), sale: frontMode ? frontRow(sale) : sale, taxDocumentInvoiceId: sale?.state === 'completed' && sale.invoice_id ? str(sale, 'invoice_id') : null, lines: frontRows(lines), payments: payments.map((payment): PosRow => ({ ...(frontMode ? frontRow(payment) : payment), provider_payment_confirmed: Boolean(payment.provider_received_at) })), transferEvidence: frontMode ? [] : transferEvidence, roomCharge: frontMode && roomCharge ? { property_id: roomCharge.property_id, guest_account_entry_id: roomCharge.guest_account_entry_id } : roomCharge, roomChargeStays, kitchen: frontRows(kitchen), refunds: frontRows(refunds), cashMovements: frontRows(cashMovements), stock: frontRows(stock), jobs: frontMode ? [] : jobs, reports: frontMode ? [] : reports, paymentMix: frontMode ? [] : paymentMix,
            companyName: (await db.readOrganizationNodes1({ company: company }))?.display_name ?? company,
            bankTransactions: frontMode ? [] : report && !branch && wants('reports') ? await db.readFinanceBankTransactions2({ company: company }) : [],
            customers: wants('sale') ? await db.posCustomers(actor) : [], receipt, companyId: company, branchId: branch,
            branches: report ? await db.readOrganizationNodes2({ company: company, branch: branch, branch2: branch }) : [],
            dayCloses: report && wants('reports') ? await db.readPosDayCloses1({ company: company, branch: branch, branch2: branch }) : [],
            accounts: frontMode ? [] : accounting ? await db.readFinanceAccounts2({ company: company }) : [],
            charts: frontMode ? [] : accounting ? await db.readFinanceCharts1({ company: company }) : [],
            bankAccounts: frontMode ? [] : bankAccounts,
            settlementConnection: !frontMode && report && !branch && wants('reports') && tenderConnection ? { account_id: tenderConnection.account_id, livemode: tenderConnection.livemode, version: tenderConnection.version } : null,
            connection: frontMode ? null : connect ? await db.readPosConnections2({ company: company }) : null,
            settlements: frontMode ? [] : report && !branch && wants('reports') ? await db.readPosSettlements1({ company: company }) : [],
            frontSetup: frontMode ? { hasProducts: Number(frontProductCount?.count ?? 0) > 0, blockedRegisters: frontBlockedRegisters } : null,
            frontReady: frontMode ? Boolean(settings?.enabled && frontHotels.length > frontBlockedRegisters.length && Number(frontProductCount?.count ?? 0) > 0) : undefined,
        };
        return frontData;
    }
    async function posExecutiveEvidenceRow(db: PosReadUnit<Effect>, actor: WorkContext, metric: PosExecutiveEvidenceMetric, recordId: string, filters: {
        from: string;
        to: string;
        currency: string;
    }) {
        ports.require(actor, hasPermission(actor,'pos.read') ? 'pos.read' : 'finance.read');
        ports.require(actor, 'pos.reports');
        if (actor.departmentId)
            throw ports.error(403, 'EVIDENCE_FORBIDDEN', 'POS evidence is available only at Company or Branch scope.');
        const [kind, id] = recordId.split(':');
        if (!id || !['sale', 'refund', 'payment'].includes(kind))
            return null;
        const company = actor.companyId, branch = actor.branchId, currency = filters.currency, from = filters.from, to = filters.to;
        if (metric === 'pos-sales' && kind === 'sale')
            return db.readPosSales3({ id: id, company: company, branch: branch, branch2: branch, currency: currency, from: from, to: to });
        if (metric === 'net-pos-sales' && kind === 'sale')
            return db.readPosSales3({ id: id, company: company, branch: branch, branch2: branch, currency: currency, from: from, to: to });
        if (metric === 'net-pos-sales' && kind === 'refund')
            return db.readPosRefunds2({ id: id, company: company, branch: branch, branch2: branch, currency: currency, from: from, to: to });
        if (metric === 'pos-payment-mix' && kind === 'payment')
            return db.readPosRefundPayments3({ id: id, company: company, branch: branch, branch2: branch, currency: currency, from: from, to: to });
        return null;
    }
    async function posWork(actor: WorkContext): Promise<import("@erpx/feature-work_projection/lib/work-item").WorkItem[]> {
        if (!hasPermission(actor, 'pos.approve') || actor.departmentId)
            return [];
        const db = await ports.forActor(actor), company = actor.companyId, branch = actor.branchId;
        const refunds = await db.readPosRefunds3({ company: company, branch: branch, branch2: branch, userId: actor.userId });
        const shifts = await db.readPosShifts1({ company: company, branch: branch, branch2: branch, userId: actor.userId });
        const cash = await db.readPosCashMovements2({ company: company, branch: branch, branch2: branch, userId: actor.userId });
        const jobs = branch ? [] : await db.readPosJobs1({ company: company });
        return [...cash.map(r => ({ id: str(r, 'id'), type: 'pos' as const, title: 'POS · Review cash drawer', detail: str(r, 'reason'), href: '/pos?tab=shifts&queue=approval', status: 'requested', updatedAt: Number(r.created_at), queue: 'todo' as const, relationships: ['approval' as const] })), ...refunds.map(r => ({ id: str(r, 'id'), type: 'pos' as const, title: 'POS · Review refund request', detail: str(r, 'reason'), href: `/pos?tab=history&queue=approval`, status: str(r, 'state'), updatedAt: Number(r.created_at), queue: 'todo' as const, relationships: ['approval' as const] })), ...shifts.map(r => ({ id: str(r, 'id'), type: 'pos' as const, title: 'POS · Review closed shift', detail: `Variance ${Number(r.variance_minor) / 100}`, href: '/pos?tab=shifts&queue=approval', status: str(r, 'state'), updatedAt: Number(r.closed_at), queue: 'todo' as const, relationships: ['approval' as const] })), ...jobs.map(r => ({ id: str(r, 'id'), type: 'pos' as const, title: 'POS · Payment item requires review', detail: str(r, 'last_error'), href: '/pos?tab=reports', status: 'exception', updatedAt: Number(r.created_at), queue: 'todo' as const, relationships: ['team' as const] }))];
    }
    /** Source receipts use the same completion, room-charge exclusion and cost allocation as the report. */
    async function posReportReceipts(actor: WorkContext, group: {
        businessDate: string;
        registerId: string;
        shiftId: string;
        currency: string;
    }, page = 1) {
        ports.require(actor, hasPermission(actor,'pos.read') ? 'pos.read' : 'finance.read');
        ports.require(actor, 'pos.reports');
        if (actor.departmentId)
            throw ports.error(403, 'POS_BRANCH_REQUIRED', 'Select a Company or Branch to read POS reports.');
        const db = await ports.forActor(actor);
        const register = scopeRow(actor, await db.readPosRegisters3({ registerId: group.registerId }));
        if (!register)
            throw ports.error(404, 'REGISTER_NOT_FOUND', 'Register not found.');
        const rows = await db.readPosLines4({ companyId: actor.companyId, companyId2: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, businessDate: group.businessDate, registerId: group.registerId, shiftId: group.shiftId, currency: group.currency, fact8: (page - 1) * 30 });
        return { row: { name: `${register.name} · ${group.businessDate}`, currency: group.currency, total_minor: rows[0]?.group_total_minor ?? 0, count: rows[0]?.group_count ?? 0 }, rows, page, hasNext: rows.length > 30 };
    }
    return { posWorkspace, posExecutiveEvidenceRow, posWork, posReportReceipts };
}
