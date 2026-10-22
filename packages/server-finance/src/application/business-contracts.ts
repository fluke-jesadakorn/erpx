import type {RecognitionItem} from "@erpx/server-finance/domain/finance-contract-policy";
import { z } from 'zod';
import { businessContractSchema, businessBillingSchema, contractBillingCalculation, type BusinessContractInput } from '@erpx/server-finance/domain/finance-contract-policy';
import { recognitionRunSchema } from '@erpx/server-finance/domain/due-inputs';
import { billingRevisionSchema } from '@erpx/server-finance/domain/business-billing-revision';
import { businessDecisionSchema, recognitionReversalSchema, billingRevisionRequest, billingHashInput, type Row, type BillingResult, type BillingRevision } from '@erpx/server-finance/domain/business-contracts';
import { invoiceSchema, type JournalInput } from '@erpx/server-finance/domain/finance';
import { decimalToMinor } from "@erpx/feature-finance/lib/finance-money";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface BusinessContractRepository<Effect> {
    contract(id: string, companyId: string): Promise<Row | null>;
    configurationChart(chartId: string, companyId: string): Promise<Row | null>;
    secondaryBook(companyId: string, chartId: string): Promise<Record<string, unknown> | null>;
    configurationAccounts(companyId: string, chartId: string, ids: string[]): Promise<{
        results: Row[];
    }>;
    customer(partyId: string, companyId: string, currency: string): Promise<Row | null>;
    dimensions(companyId: string): Promise<{
        results: Row[];
    }>;
    dimensionValue(companyId: string, id: string | number | null, value: string): Promise<Record<string, unknown> | null>;
    taxCode(companyId: string, taxCode: string | undefined, taxCode2: string | undefined, startDate: string, startDate2: string): Promise<Row | null>;
    billedWindow(root_id: string | number | null, startDate: string): Promise<Record<string, unknown> | null>;
    guardContractParent(key: string, value: "contract-parent", id: string | number | null, companyId: string, value2: number): Effect;
    prepareContract(id: string, companyId: string, root: string, value: string | number | null, number: string, name: string, chartId: string, partyId: string, startDate: string, endDate: string, edition: number, value2: string, sourceHash: string, userId: string, reason: string, now: number, now2: number): Effect;
    retainedBilledWindow(root_id: string | number | null, startDate: string): Promise<Record<string, unknown> | null>;
    guardContractVersion(key: string, value: "contract-version", id: string, companyId: string, status: "draft" | "active", expectedVersion: number): Effect;
    supersedeParent(now: number, id: string | number | null): Effect;
    decideContract(status: "active" | "rejected" | "cancelled", userId: string, version: number, now: number, id: string): Effect;
    contractDecisionEvidence(value: string, key: string, id: string, op: string, userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number): Effect;
    postedOwnerCredit(postedCorrectionId: string, companyId: string, userId: string, id: string): Promise<Row | null>;
    parentBill(parentId: unknown, companyId: string, contractId: string): Promise<Row | null>;
    childBill(parentId: unknown): Promise<Record<string, unknown> | null>;
    recognitionExists(id: string | number | null): Promise<Record<string, unknown> | null>;
    billingExists(root_id: string | number | null, billingKey: string): Promise<Record<string, unknown> | null>;
    guardBillParent(key: string, value: "billing-parent-version", id: string | number | null, companyId: string, version: string | number | null, id2: string | number | null): Effect;
    guardParentInvoice(key: string, value: "billing-parent-invoice", invoice_id: string | number, companyId: string, original_invoice_version: string | number | null): Effect;
    voidParentInvoice(now: number, invoice_id: string | number, companyId: string): Effect;
    guardBillingContract(key: string, value: "contract-version", contractId: string, companyId: string, version: number, retainedEdition: boolean): Effect;
    prepareBill(id: string, companyId: string, contractId: string, root_id: string | number | null, version: number, billingKey: string, serviceFrom: string, serviceTo: string, value: string, value2: string, sourceHash: string, value3: string | null, value4: number | null, value5: string | number, now: number, value6: "draft" | null, value7: string | number | null, value8: string | number | null, value9: number, value10: string | null, value11: string | null): Effect;
    billContractId(parentId: string, companyId: string): Promise<{
        contract_id: string;
    } | null>;
    billSource(billingId: string, companyId: string): Promise<Row | null>;
    postedCorrection(id: string | number | null): Promise<Record<string, unknown> | null>;
    recognitionInvoice(invoice_id: string | number | null, companyId: string): Promise<Row | null>;
    openPeriod(periodId: unknown, companyId: string, entryDate: unknown, entryDate2: unknown): Promise<Record<string, unknown> | null>;
    recognitionExistsItem(billingRunId: unknown, itemKey: unknown): Promise<Record<string, unknown> | null>;
    prepareRecognition(id: string, companyId: string, billingRunId: unknown, itemKey: unknown, date: string, entryDate: unknown, periodId: unknown, value: number, accountId: string, sourceHash: string, userId: string, reason: unknown, now: number, now2: number): Effect;
    recognition(id: string, companyId: string): Promise<Row | null>;
    guardRecognitionDraft(key: string, value: "recognition-version", id: string, companyId: string, expectedVersion: number): Effect;
    decideRecognition(status: "rejected" | "posted", version: number, userId: string, value: string | null, reason: string, now: number, id: string): Effect;
    recognitionDecisionEvidence(value: string, key: string, id: string, op: string, userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number): Effect;
    postedRecognition(id: string, companyId: string): Promise<Row | null>;
    guardPostedRecognition(key: string, value: "recognition-version", id: string, companyId: string, expectedVersion: number): Effect;
    reverseRecognition(reversalId: string, reason: string, now: number, id: string): Effect;
    zeroBill(id: string, companyId: string): Promise<Row | null>;
    zeroChildBill(id: string): Promise<Record<string, unknown> | null>;
    zeroParentBill(parent_run_id: string | number, companyId: string): Promise<Row | null>;
    guardZeroBill(key: string, value: "zero-usage-version", id: string, companyId: string, expectedVersion: number, id2: string): Effect;
    decideZeroBill(value: "rejected" | "approved", userId: string, reason: string, id: string): Effect;
    workspaceContracts(companyId: string): Promise<{
        results: Row[];
    }>;
    workspaceBills(companyId: string): Promise<{
        results: Row[];
    }>;
    workspaceRecognitions(companyId: string): Promise<{
        results: Row[];
    }>;
    workspaceAccounts(companyId: string): Promise<{
        results: Row[];
    }>;
    workspaceCustomers(companyId: string): Promise<{
        results: Row[];
    }>;
    workspaceCharts(companyId: string): Promise<{
        results: Row[];
    }>;
    workspacePeriods(companyId: string): Promise<{
        results: Row[];
    }>;
    workspaceCorrections(companyId: string): Promise<{
        results: Row[];
    }>;
}
export interface BusinessContractPorts<Effect> {
    repository: BusinessContractRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OwnerCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    hash(value: string): Promise<string>;
    requirePermission(actor: WorkContext, permission: 'finance.read' | 'finance.write' | 'finance.approve'): WorkContext;
    requireMoneyScale(companyId: string, currency: string): Promise<unknown>;
    requireTool(companyId: string, tool: 'business_contracts'): Promise<unknown>;
    transaction<Result>(work: (ports: BusinessContractPorts<Effect>) => Promise<Result>): Promise<Result>;
    ownedTransaction<Result>(available: boolean, work: (ports: BusinessContractPorts<Effect>) => Promise<Result>): Promise<Result>;
    createInvoice(actor: WorkContext, input: z.infer<typeof invoiceSchema>, key: string): Promise<{
        id: string;
        version: number;
    }>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string, status: 'posted'): Promise<{
        result: {
            id: string;
        };
        steps: Effect[];
    }>;
    prepareReversal(actor: WorkContext, journalId: string, sourceType: 'business_recognition', sourceId: string, input: {
        periodId: string;
        entryDate: string;
        reason: string;
    }, key: string): Promise<{
        id: string;
        steps: Effect[];
    }>;
}
export function createBusinessContractApplication<Effect>(ports: BusinessContractPorts<Effect>) {
    const r = ports.repository;
    async function contract(actor: WorkContext, id: string) {
        const row = await r.contract(id, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "CONTRACT_NOT_FOUND", "Business contract not found.");
        return row;
    }
    async function businessContractConfiguration(actor: WorkContext, input: BusinessContractInput) {
        await ports.requireMoneyScale(actor.companyId, input.currency);
        const chart = await r.configurationChart(input.chartId, actor.companyId);
        if (!chart ||
            chart.status !== "active" ||
            chart.base_currency !== input.currency)
            throw new BusinessRuleError(422, "CONTRACT_BOOK_INVALID", "Use an active primary book and its functional currency for business billing.");
        if (await r.secondaryBook(actor.companyId, input.chartId))
            throw new BusinessRuleError(422, "CONTRACT_PRIMARY_BOOK_REQUIRED", "Business invoices belong to the primary book.");
        const ids = [
            ...new Set([
                input.receivableAccountId,
                input.deferredAccountId,
                ...input.obligations.map((o) => o.revenueAccountId),
                ...(input.taxBps ? [input.taxAccountId!] : []),
            ]),
        ];
        const accounts = (await r.configurationAccounts(actor.companyId, input.chartId, ids)).results;
        const byId = new Map(accounts.map((a) => [a.id, a]));
        if (accounts.length !== ids.length ||
            accounts.some((a) => a.currency !== input.currency) ||
            byId.get(input.receivableAccountId)?.control_kind !== "receivable" ||
            byId.get(input.receivableAccountId)?.category !== "asset" ||
            byId.get(input.deferredAccountId)?.category !== "liability" ||
            input.obligations.some((o) => byId.get(o.revenueAccountId)?.category !== "revenue") ||
            input.receivableAccountId === input.deferredAccountId ||
            input.obligations.some((o) => [input.receivableAccountId, input.deferredAccountId].includes(o.revenueAccountId)) ||
            (input.taxBps &&
                [
                    input.receivableAccountId,
                    input.deferredAccountId,
                    ...input.obligations.map((o) => o.revenueAccountId),
                ].includes(input.taxAccountId!)))
            throw new BusinessRuleError(422, "CONTRACT_ACCOUNT_MAPPING_REQUIRED", "Map distinct receivable, deferred revenue, income and VAT accounts in this book.");
        const party = await r.customer(input.partyId, actor.companyId, input.currency);
        if (!party)
            throw new BusinessRuleError(422, "CONTRACT_CUSTOMER_REQUIRED", "Choose an active customer with this invoice currency.");
        const dims = (await r.dimensions(actor.companyId)).results;
        if (Object.keys(input.dimensions).some((key) => !dims.some((d) => d.code === key)))
            throw new BusinessRuleError(422, "DIMENSION_VALUE_REQUIRED", "Use registered active reporting dimensions for the contract.");
        for (const d of dims) {
            const value = input.dimensions[String(d.code)];
            if ((d.required && !value) ||
                (value &&
                    !(await r.dimensionValue(actor.companyId, d.id, value))))
                throw new BusinessRuleError(422, "DIMENSION_VALUE_REQUIRED", "Complete the Company's active reporting dimensions.");
        }
        let taxCode: Row | null = null;
        if (input.taxBps) {
            taxCode = await r.taxCode(actor.companyId, input.taxCode, input.taxCode, input.startDate, input.startDate);
            if (!taxCode)
                throw new BusinessRuleError(422, "CONTRACT_TAX_CODE_REQUIRED", "Choose the active ordinary 7% Thai VAT code effective at contract commencement.");
        }
        return { chart, accounts, party, taxCode, dimensions: input.dimensions };
    }
    async function prepareBusinessContract(actor: WorkContext, raw: z.input<typeof businessContractSchema>, key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.write");
        const input = businessContractSchema.parse(raw), id = `business-contract-${key}`, op = "finance.business-contract.prepare";
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, r = ports.repository;
            const { contract, businessContractConfiguration } = createBusinessContractApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; edition: number; sourceHash: string; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            await ports.requireTool(actor.companyId, "business_contracts");
            const parent = input.parentId
                ? await contract(actor, input.parentId)
                : null;
            if (parent &&
                (parent.status !== "active" ||
                    parent.version !== input.expectedVersion ||
                    parent.number !== input.number ||
                    parent.party_id !== input.partyId ||
                    parent.chart_id !== input.chartId))
                throw new BusinessRuleError(409, "CONTRACT_AMENDMENT_CHANGED", "Amend the current active edition while retaining its customer, number and primary book.");
            if (parent &&
                (await r.billedWindow(parent.root_id, input.startDate)))
                throw new BusinessRuleError(409, "CONTRACT_BILLED_WINDOW", "An amendment or renewal must begin after retained billed service windows. Correct existing invoices separately.");
            const source = await businessContractConfiguration(actor, input), sourceHash = await ports.hash(JSON.stringify({ input, source })), now = ports.clock.now(), edition = Number(parent?.edition ?? 0) + 1, root = String(parent?.root_id ?? id);
            return ports.commands.commit(actor, key, op, input, id, 1, { id, status: "draft", version: 1, edition, sourceHash }, [
                ...(parent
                    ? [
                        r.guardContractParent(key, "contract-parent", parent.id, actor.companyId, input.expectedVersion ?? 0),
                    ]
                    : []),
                r.prepareContract(id, actor.companyId, root, parent?.id ?? null, input.number, input.name, input.chartId, input.partyId, input.startDate, input.endDate, edition, JSON.stringify(input), sourceHash, actor.userId, input.reason, now, now),
            ]);
        });
    }
    async function decideBusinessContract(actor: WorkContext, id: string, raw: z.input<typeof businessDecisionSchema>, action: "approve" | "reject" | "cancel", key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.approve");
        const input = businessDecisionSchema.parse(raw), op = `finance.business-contract.${action}`;
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, r = ports.repository;
            const { contract, businessContractConfiguration } = createBusinessContractApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const row = await contract(actor, id);
            if (row.version !== input.expectedVersion ||
                row.status !== (action === "cancel" ? "active" : "draft"))
                throw new BusinessRuleError(409, "CONTRACT_CHANGED", "Refresh the contract edition before deciding.");
            if (samePerson(actor, row.prepared_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "Another Finance approver must decide this contract.");
            const content = businessContractSchema.parse(JSON.parse(String(row.content_json))), parent = row.parent_id
                ? await contract(actor, String(row.parent_id))
                : null;
            if (action === "approve") {
                const source = await businessContractConfiguration(actor, content);
                if ((await ports.hash(JSON.stringify({ input: content, source }))) !==
                    row.source_hash)
                    throw new BusinessRuleError(409, "CONTRACT_SOURCE_CHANGED", "Contract accounts or customer evidence changed. Reject this draft and prepare a fresh edition.");
                if (parent &&
                    (parent.status !== "active" ||
                        parent.version !== content.expectedVersion ||
                        (await r.retainedBilledWindow(parent.root_id, content.startDate))))
                    throw new BusinessRuleError(409, "CONTRACT_AMENDMENT_CHANGED", "The previous edition or its billed windows changed.");
            }
            const status = action === "approve"
                ? "active"
                : action === "reject"
                    ? "rejected"
                    : "cancelled", version = input.expectedVersion + 1;
            return ports.commands.commit(actor, key, op, input, id, version, { id, status, version }, [
                r.guardContractVersion(key, "contract-version", id, actor.companyId, row.status, input.expectedVersion),
                ...(action === "approve" && parent
                    ? [
                        r.supersedeParent(ports.clock.now(), parent.id),
                    ]
                    : []),
                r.decideContract(status, actor.userId, version, ports.clock.now(), id),
                r.contractDecisionEvidence(ports.identifiers.next(), key, id, op, actor.userId, actor.assignmentId, input.expectedVersion, await ports.hash(JSON.stringify(input)), ports.clock.now()),
            ]);
        });
    }
    async function prepareBusinessBilling(actor: WorkContext, contractId: string, raw: z.input<typeof businessBillingSchema>, key: string, ownedTransaction = false, revision?: BillingRevision, postedCorrectionId?: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.write");
        const input = businessBillingSchema.parse(raw), id = `business-bill-${key}`, op = revision
            ? "finance.business-billing.revise"
            : "finance.business-billing.prepare", request = revision
            ? billingRevisionRequest(revision)
            : { contractId, ...input };
        await ports.prepare(actor.holdingId);
        return ports.ownedTransaction(ownedTransaction, async (transactionPorts) => {
            const ports = transactionPorts, r = ports.repository;
            const { contract, businessContractConfiguration } = createBusinessContractApplication(ports);
            const prior = await ports.commands.replay<{ id: string; invoiceId: string | null; sourceHash: string; result: { serviceFrom: string; serviceTo: string; billingKey: string; subtotal: string; tax: string; total: string; allocations: { key: string; amount: string; }[]; items: RecognitionItem[]; }; status: string; }>(actor, key, op, id, request);
            if (prior)
                return prior;
            await ports.requireTool(actor.companyId, "business_contracts");
            const row = await contract(actor, contractId);
            if ((!revision && row.status !== "active") ||
                (revision &&
                    !["active", "cancelled", "superseded"].includes(String(row.status))) ||
                row.version !== input.expectedVersion)
                throw new BusinessRuleError(409, "CONTRACT_CHANGED", "Billing requires the current independently approved contract edition.");
            let parent: Row | undefined;
            const ownerCredit = postedCorrectionId ? await r.postedOwnerCredit(postedCorrectionId, actor.companyId, actor.userId, id) : null;
            if (postedCorrectionId && (!ownedTransaction || !ownerCredit || !revision || ownerCredit.billing_run_id !== revision.parentId || samePerson(actor, ownerCredit.prepared_by)))
                throw new BusinessRuleError(409, 'BILLING_CREDIT_OWNER_REQUIRED', 'Replacement billing requires the independent credit owner transaction.');
            if (revision) {
                parent =
                    (await r.parentBill(revision.parentId, actor.companyId, contractId)) ?? undefined;
                if (!parent ||
                    parent.version !== revision.expectedVersion ||
                    (await r.childBill(revision.parentId)))
                    throw new BusinessRuleError(409, "BILLING_REVISION_SOURCE_CHANGED", "Choose the current retained billing edition and refresh its version.");
                if (parent.invoice_id && parent.original_invoice_version !== revision.invoiceExpectedVersion)
                    throw new BusinessRuleError(409, 'BILLING_INVOICE_CHANGED', 'Refresh the current original invoice version before preparing a correction.');
                if (!ownerCredit &&
                    parent.invoice_id &&
                    (!["draft", "voided"].includes(String(parent.original_invoice_status)) ||
                        parent.original_journal_id ||
                        parent.original_posted_at ||
                        parent.original_invoice_version !== revision.invoiceExpectedVersion))
                    throw new BusinessRuleError(409, "BILLING_POSTED_CORRECTION_REQUIRED", "A posted or paid bill requires a reviewed credit/debit correction through its accounting and tax owners. This revision applies to an unposted bill.");
                if (await r.recognitionExists(parent.id))
                    throw new BusinessRuleError(409, "BILLING_POSTED_CORRECTION_REQUIRED", "Resolve the owned revenue recognition before preparing a billing correction.");
                if (input.invoiceDate < JSON.parse(String(parent.input_json)).invoiceDate)
                    throw new BusinessRuleError(422, "BILLING_REVISION_DATE_INVALID", "Date the corrected bill on or after its previous invoice date.");
            }
            const content = businessContractSchema.parse({
                ...JSON.parse(String(parent?.contract_content_json ?? row.content_json)),
                ...(revision?.pricing ? { pricing: revision.pricing } : {}),
                ...(revision?.obligations ? { obligations: revision.obligations } : {}),
            });
            await businessContractConfiguration(actor, content);
            let result: BillingResult;
            try {
                result = contractBillingCalculation(content, input);
            }
            catch (e) {
                throw new BusinessRuleError(422, "BILLING_INPUT_INVALID", e instanceof Error ? e.message : "Invalid billing input.");
            }
            const noCharge = decimalToMinor(result.subtotal) === 0;
            if (parent &&
                (result.billingKey !== parent.billing_key ||
                    result.serviceFrom !== parent.service_from ||
                    result.serviceTo !== parent.service_to))
                throw new BusinessRuleError(422, "BILLING_REVISION_WINDOW_CHANGED", "Retain this original service window. Prepare a different month through the normal billing workflow.");
            if (!parent &&
                (await r.billingExists(row.root_id, result.billingKey)))
                throw new BusinessRuleError(409, "BILLING_ALREADY_PREPARED", "This service window already has a retained billing run.");
            const invoice = noCharge
                ? null
                : await ports.createInvoice(actor, invoiceSchema.parse({
                    partyId: content.partyId,
                    kind: "receivable",
                    invoiceNumber: parent
                        ? `${content.number.slice(0, 60)}-${result.billingKey}-b${Number(parent.bill_edition) + 1}`
                        : `${content.number}-${result.billingKey}-e${row.edition}`,
                    invoiceDate: input.invoiceDate,
                    dueDate: input.dueDate,
                    currency: content.currency,
                    subtotal: result.subtotal,
                    tax: result.tax,
                    taxCode: content.taxCode,
                    taxAccountId: content.taxAccountId,
                    debitAccountId: content.receivableAccountId,
                    creditAccountId: content.deferredAccountId,
                }), `${key}-invoice`);
            const sourceHash = await ports.hash(JSON.stringify(billingHashInput(row, input, result, parent, content, revision?.reason))), now = ports.clock.now();
            return ports.commands.commit(actor, key, op, request, id, 1, {
                id,
                invoiceId: invoice?.id ?? null,
                sourceHash,
                result,
                status: noCharge ? "no_charge_draft" : "draft",
            }, [
                ...(parent
                    ? [
                        r.guardBillParent(key, "billing-parent-version", parent.id, actor.companyId, parent.version, parent.id),
                        ...(parent.invoice_id && !ownerCredit
                            ? [
                                r.guardParentInvoice(key, "billing-parent-invoice", parent.invoice_id, actor.companyId, parent.original_invoice_version),
                                r.voidParentInvoice(now, parent.invoice_id, actor.companyId),
                            ]
                            : []),
                    ]
                    : []),
                r.guardBillingContract(key, "contract-version", contractId, actor.companyId, row.version, Boolean(parent)),
                r.prepareBill(id, actor.companyId, contractId, row.root_id, row.version, result.billingKey, result.serviceFrom, result.serviceTo, JSON.stringify(input), JSON.stringify(result), sourceHash, invoice?.id ?? null, invoice?.version ?? null, ownerCredit?.prepared_by ?? actor.userId, now, noCharge ? "draft" : null, parent?.id ?? null, parent?.version ?? null, parent ? Number(parent.bill_edition) + 1 : 1, parent ? JSON.stringify(content) : null, revision?.reason ?? null),
            ]);
        });
    }
    async function prepareBusinessBillingRevision(actor: WorkContext, parentId: string, raw: z.input<typeof billingRevisionSchema>, key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.write");
        const input = billingRevisionSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, r = ports.repository;
            const { contract, prepareBusinessBilling } = createBusinessContractApplication(ports);
            const previous = await ports.commands.replay(actor, key, "finance.business-billing.revise", `business-bill-${key}`, { parentId, ...input });
            if (previous)
                return previous;
            const parent = await r.billContractId(parentId, actor.companyId);
            if (!parent)
                throw new BusinessRuleError(404, "BILLING_RUN_NOT_FOUND", "Billing run not found in this Company.");
            const c = await contract(actor, parent.contract_id);
            return prepareBusinessBilling(actor, parent.contract_id, { ...input.billing, expectedVersion: Number(c.version) }, key, true, { parentId, ...input });
        });
    }
    async function recognitionSource(actor: WorkContext, billingId: string, itemKey: string) {
        const bill = await r.billSource(billingId, actor.companyId);
        if (!bill)
            throw new BusinessRuleError(404, "BILLING_RUN_NOT_FOUND", "Billing run not found.");
        const content = businessContractSchema.parse(JSON.parse(String(bill.content_json))), result = JSON.parse(String(bill.result_json)) as BillingResult, item = result.items.find((i) => i.key === itemKey);
        if (!item || decimalToMinor(item.amount) <= 0)
            throw new BusinessRuleError(422, "RECOGNITION_ITEM_REQUIRED", "Choose a positive retained recognition item.");
        if (await r.postedCorrection(bill.id))
            throw new BusinessRuleError(409, 'RECOGNITION_BILL_CREDITED', 'Use the corrected billing edition for recognition.');
        const invoice = await r.recognitionInvoice(bill.invoice_id, actor.companyId);
        if (!invoice ||
            !["posted", "partially_paid", "paid"].includes(String(invoice.status)) ||
            invoice.journal_status !== "posted" ||
            Number(invoice.line_revision) !== 1 ||
            Number(invoice.subtotal_minor) !== decimalToMinor(result.subtotal) ||
            invoice.debit_account_id !== content.receivableAccountId ||
            invoice.credit_account_id !== content.deferredAccountId)
            throw new BusinessRuleError(409, "RECOGNITION_INVOICE_REQUIRED", "Independently post the original billed invoice before recognizing revenue.");
        const config = await businessContractConfiguration(actor, content), sourceHash = await ports.hash(JSON.stringify({
            billingHash: bill.source_hash,
            item,
            invoice: { ...invoice, status: "posted" },
            config,
        }));
        return { bill, content, item, invoice, sourceHash };
    }
    async function prepareRecognitionRun(actor: WorkContext, raw: z.input<typeof recognitionRunSchema>, key: string, ownedTransaction = false): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.write");
        const input = recognitionRunSchema.parse(raw), id = `recognition-${key}`, op = "finance.recognition.prepare";
        await ports.prepare(actor.holdingId);
        return ports.ownedTransaction(ownedTransaction, async (transactionPorts) => {
            const ports = transactionPorts, r = ports.repository;
            const { recognitionSource } = createBusinessContractApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; amount: string; sourceHash: string; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            await ports.requireTool(actor.companyId, "business_contracts");
            const source = await recognitionSource(actor, input.billingRunId, input.itemKey);
            if (input.entryDate < source.item.date ||
                input.entryDate < String(source.invoice.posted_date))
                throw new BusinessRuleError(422, "RECOGNITION_DATE_INVALID", "Recognition cannot precede its due date or the posted deferral.");
            if (!(await r.openPeriod(input.periodId, actor.companyId, input.entryDate, input.entryDate)))
                throw new BusinessRuleError(409, "PERIOD_NOT_OPEN", "Choose an open accounting period covering this recognition date.");
            if (await r.recognitionExistsItem(input.billingRunId, input.itemKey))
                throw new BusinessRuleError(409, "RECOGNITION_ALREADY_PREPARED", "This recognition item already has current evidence.");
            return ports.commands.commit(actor, key, op, input, id, 1, {
                id,
                status: "draft",
                version: 1,
                amount: source.item.amount,
                sourceHash: source.sourceHash,
            }, [
                r.prepareRecognition(id, actor.companyId, input.billingRunId, input.itemKey, source.item.date, input.entryDate, input.periodId, decimalToMinor(source.item.amount), source.item.accountId, source.sourceHash, actor.userId, input.reason, ports.clock.now(), ports.clock.now()),
            ]);
        });
    }
    async function decideRecognitionRun(actor: WorkContext, id: string, raw: z.input<typeof businessDecisionSchema>, action: "post" | "reject", key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.approve");
        const input = businessDecisionSchema.parse(raw), op = `finance.recognition.${action}`;
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, r = ports.repository;
            const { recognitionSource } = createBusinessContractApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; journalId: string | null; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const row = await r.recognition(id, actor.companyId);
            if (!row)
                throw new BusinessRuleError(404, "RECOGNITION_NOT_FOUND", "Recognition run not found.");
            if (row.status !== "draft" || row.version !== input.expectedVersion)
                throw new BusinessRuleError(409, "RECOGNITION_CHANGED", "Refresh the draft recognition before deciding.");
            if (samePerson(actor, row.prepared_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "Another Finance approver must decide recognition.");
            const source = action === "post"
                ? await recognitionSource(actor, String(row.billing_run_id), String(row.item_key))
                : null;
            if (source && source.sourceHash !== row.source_hash)
                throw new BusinessRuleError(409, "RECOGNITION_SOURCE_CHANGED", "Invoice or account evidence changed. Reject and prepare a fresh recognition run.");
            const journal = source
                ? await ports.prepareJournal(actor, {
                    chartId: source.content.chartId,
                    periodId: String(row.period_id),
                    entryDate: String(row.entry_date),
                    currency: source.content.currency,
                    memo: `Revenue recognition ${source.content.number}: ${source.item.name}`,
                    sourceType: "business_recognition",
                    sourceId: id,
                    isOpening: false,
                    lines: [
                        {
                            accountId: source.content.deferredAccountId,
                            debit: source.item.amount,
                            credit: "0",
                            dimensions: source.content.dimensions,
                        },
                        {
                            accountId: source.item.accountId,
                            debit: "0",
                            credit: source.item.amount,
                            dimensions: source.content.dimensions,
                        },
                    ],
                }, key, "posted")
                : null;
            const status = action === "post" ? "posted" : "rejected", version = input.expectedVersion + 1;
            return ports.commands.commit(actor, key, op, input, id, version, { id, status, version, journalId: journal?.result.id ?? null }, [
                r.guardRecognitionDraft(key, "recognition-version", id, actor.companyId, input.expectedVersion),
                ...(journal?.steps ?? []),
                r.decideRecognition(status, version, actor.userId, journal?.result.id ?? null, input.reason, ports.clock.now(), id),
                r.recognitionDecisionEvidence(ports.identifiers.next(), key, id, op, actor.userId, actor.assignmentId, input.expectedVersion, await ports.hash(JSON.stringify(input)), ports.clock.now()),
            ]);
        });
    }
    async function reverseRecognitionRun(actor: WorkContext, id: string, raw: z.input<typeof recognitionReversalSchema>, key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.approve");
        const input = recognitionReversalSchema.parse(raw), op = "finance.recognition.reverse";
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, r = ports.repository;
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; reversalJournalId: string; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const row = await r.postedRecognition(id, actor.companyId);
            if (!row || row.version !== input.expectedVersion)
                throw new BusinessRuleError(409, "RECOGNITION_CHANGED", "Refresh the posted recognition before reversing it.");
            if (samePerson(actor, row.prepared_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "Another Finance approver must reverse recognition.");
            if (input.entryDate < String(row.entry_date))
                throw new BusinessRuleError(422, "RECOGNITION_REVERSAL_DATE", "Reverse on or after the original recognition date.");
            const reversal = await ports.prepareReversal(actor, String(row.journal_id), "business_recognition", id, input, key);
            const reversalId = reversal.id;
            return ports.commands.commit(actor, key, op, input, id, input.expectedVersion + 1, {
                id,
                status: "reversed",
                version: input.expectedVersion + 1,
                reversalJournalId: reversalId,
            }, [
                r.guardPostedRecognition(key, "recognition-version", id, actor.companyId, input.expectedVersion),
                ...reversal.steps,
                r.reverseRecognition(reversalId, input.reason, ports.clock.now(), id),
            ]);
        });
    }
    async function decideNoChargeBilling(actor: WorkContext, id: string, raw: z.input<typeof businessDecisionSchema>, action: "approve" | "reject", key: string): Promise<Record<string, unknown>> {
        actor = ports.requirePermission(actor, "finance.approve");
        const input = businessDecisionSchema.parse(raw), op = `finance.business-zero.${action}`;
        await ports.prepare(actor.holdingId);
        return ports.transaction(async (transactionPorts) => {
            const ports = transactionPorts, r = ports.repository;
            const { contract, businessContractConfiguration } = createBusinessContractApplication(ports);
            const prior = await ports.commands.replay<{ id: string; status: string; version: number; }>(actor, key, op, id, input);
            if (prior)
                return prior;
            const row = await r.zeroBill(id, actor.companyId);
            if (!row ||
                row.version !== input.expectedVersion ||
                row.no_charge_status !== "draft" ||
                (await r.zeroChildBill(id)))
                throw new BusinessRuleError(409, "ZERO_USAGE_BILL_CHANGED", "Refresh the current zero-usage edition before deciding.");
            if (samePerson(actor, row.created_by))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "Another Finance approver must review the zero-usage evidence.");
            if (action === "approve") {
                const c = await contract(actor, String(row.contract_id)), content = businessContractSchema.parse(JSON.parse(String(row.contract_content_json ?? c.content_json))), billing = businessBillingSchema.parse(JSON.parse(String(row.input_json))), result = contractBillingCalculation(content, billing);
                await businessContractConfiguration(actor, content);
                const parent = row.parent_run_id
                    ? ((await r.zeroParentBill(row.parent_run_id, actor.companyId)) ?? undefined)
                    : undefined;
                if (row.parent_run_id &&
                    (!parent || parent.version !== row.parent_version))
                    throw new BusinessRuleError(409, "BILLING_REVISION_SOURCE_CHANGED", "The retained previous billing edition changed. Prepare a fresh correction.");
                const hash = await ports.hash(JSON.stringify(billingHashInput({ ...c, version: row.contract_version }, billing, result, parent, content, String(row.revision_reason))));
                if ((row.parent_run_id
                    ? !["active", "cancelled", "superseded"].includes(String(c.status))
                    : c.status !== "active") ||
                    c.version !== row.contract_version ||
                    hash !== row.source_hash ||
                    result.subtotal !== "0.00" ||
                    result.tax !== "0.00")
                    throw new BusinessRuleError(409, "ZERO_USAGE_EVIDENCE_CHANGED", "Reproduce this retained zero-usage edition and its approved contract evidence before review.");
            }
            return ports.commands.commit(actor, key, op, input, id, input.expectedVersion + 1, {
                id,
                status: action === "approve" ? "approved" : "rejected",
                version: input.expectedVersion + 1,
            }, [
                r.guardZeroBill(key, "zero-usage-version", id, actor.companyId, input.expectedVersion, id),
                r.decideZeroBill(action === "approve" ? "approved" : "rejected", actor.userId, input.reason, id),
            ]);
        });
    }
    async function businessContractWorkspace(actor: WorkContext) {
        actor = ports.requirePermission(actor, "finance.read");
        await ports.prepare(actor.holdingId);
        const [contracts, bills, recognitions, accounts, parties, charts, periods, corrections] = await Promise.all([
            r.workspaceContracts(actor.companyId),
            r.workspaceBills(actor.companyId),
            r.workspaceRecognitions(actor.companyId),
            r.workspaceAccounts(actor.companyId),
            r.workspaceCustomers(actor.companyId),
            r.workspaceCharts(actor.companyId),
            r.workspacePeriods(actor.companyId),
            r.workspaceCorrections(actor.companyId),
        ]);
        return {
            corrections: corrections.results,
            contracts: contracts.results,
            bills: bills.results,
            recognitions: recognitions.results,
            accounts: accounts.results,
            parties: parties.results,
            charts: charts.results,
            periods: periods.results,
        };
    }
    return { contract, businessContractConfiguration, recognitionSource, prepareBusinessContract, decideBusinessContract, prepareBusinessBilling, prepareBusinessBillingRevision, prepareRecognitionRun, decideRecognitionRun, reverseRecognitionRun, decideNoChargeBilling, businessContractWorkspace };
}


