import { z } from 'zod';
import { samePerson } from '@/lib/iam';
import { minorToDecimal } from '@/lib/finance-money';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
import { createVatTaxEntryPlan, type VatTaxEntryPlan, month, monthDates, vatEntrySchema, taxReviewSchema, vatCorrectionSchema, vatPackageSchema, vatFilingSchema, createVatFactsPolicy, type TaxRow, type VatEntryInput, type VatReviewInput, type VatCorrectionInput, type VatPackageInput, type VatFilingInput, type VatSourceType, type VatSource, type VatRegister, type VatReconciliation, type VatCorrectionMember, type VatResult, type VatSnapshot, type VatCloseEvidence, type VatCorrectionDetail, type VatEntryResult, type VatReviewResult, type VatCorrectionResult, type VatPackageResult, type VatExportResult } from '../domain/vat-register';
export interface VatRegisterPorts {
    error: FinanceError;
    permission(actor: WorkContext, permission: "finance.read" | "finance.write" | "finance.approve"): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    digest(value: unknown): string;
    hashBytes(value: string): string;
    replay<Result>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    vatSource(actor: WorkContext, type: VatSourceType, id: string): Promise<TaxRow | null>;
    profile(actor: WorkContext, chartId: string, date: string): Promise<TaxRow | null>;
    package(actor: WorkContext, id: string): Promise<TaxRow | null>;
    correction(actor: WorkContext, id: string): Promise<TaxRow | null>;
    originalVatEntry(actor: WorkContext, source: TaxRow): Promise<TaxRow | null>;
    familyEvidence(actor: WorkContext, id: string): Promise<string>;
    fullRegister(actor: WorkContext, chartId: string, month: string): Promise<string | null>;
    reconciliationRows(actor: WorkContext, chartId: string, start: string, end: string): Promise<TaxRow[]>;
    missingSources(actor: WorkContext, chartId: string, start: string, end: string): Promise<number | null>;
    pendingCorrectionCount(actor: WorkContext, chartId: string, end: string): Promise<number | null>;
    commitPrepareVatEntry(facts: {
        actor: WorkContext;
        key: string;
        input: VatEntryInput;
        id: string;
        ledger: number;
        deductible: number;
        source: TaxRow;
        familyId: string;
        taxEntry: VatTaxEntryPlan;
    }): Promise<VatEntryResult>;
    entry(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<TaxRow | null>;
    commitReviewVatEntry(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: VatReviewInput;
        id: string;
        status: 'approved' | 'rejected';
        verifiedSource: VatSource | null;
        entry: TaxRow;
    }): Promise<VatReviewResult>;
    previousEntry(facts: {
        entryId: string;
        actor: WorkContext;
    }): Promise<TaxRow | null>;
    pendingFamily(facts: {
        actor: WorkContext;
        familyId: string;
    }): Promise<unknown | null>;
    commitPrepareVatCorrection(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        entryId: string;
        input: VatCorrectionInput;
        id: string;
        members: VatCorrectionMember[];
        taxEntries: VatTaxEntryPlan[];
        familyId: string;
        json: string;
        previous: TaxRow;
    }): Promise<VatCorrectionResult>;
    replacementEntries(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<TaxRow[]>;
    commitReviewVatCorrection(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: VatReviewInput;
        id: string;
        status: 'approved' | 'rejected';
        entries: TaxRow[];
        verifiedFamilyJson: string | null;
        correction: TaxRow;
        verifiedSources: VatSource[];
        now: number;
    }): Promise<VatCorrectionResult>;
    commitPrepareVatPackage(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: VatPackageInput;
        id: string;
        result: VatResult;
        r: VatRegister;
        dates: {
            start: string;
            end: string;
        };
        p: TaxRow;
        reconciliation: VatReconciliation;
        runId: string;
        now: number;
        snapshot: VatSnapshot;
        original: TaxRow | null;
    }): Promise<VatPackageResult>;
    commitApproveVatPackage(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: VatReviewInput;
        id: string;
        p: TaxRow;
        r: VatRegister;
        currentProfile: TaxRow;
        current: VatReconciliation;
    }): Promise<VatReviewResult>;
    commitExportVatWorkpaper(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: VatReviewInput;
        id: string;
        p: TaxRow;
        content: string;
        exportHash: string;
        r: VatRegister;
        currentProfile: TaxRow;
        current: VatReconciliation;
    }): Promise<VatExportResult>;
    commitRecordVatFiling(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: VatFilingInput;
        id: string;
    }): Promise<VatReviewResult>;
    commitSupersedeVatPackage(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: VatReviewInput;
        id: string;
    }): Promise<VatReviewResult>;
    profiles(facts: {
        actor: WorkContext;
    }): Promise<TaxRow[]>;
    chart(facts: {
        chartId: string;
        actor: WorkContext;
    }): Promise<unknown | null>;
    workspaceEntries(facts: {
        actor: WorkContext;
        chartId: string;
        filingMonth: string;
    }): Promise<TaxRow[]>;
    workspaceCandidates(facts: {
        actor: WorkContext;
        chartId: string;
    }): Promise<TaxRow[]>;
    workspacePackages(facts: {
        actor: WorkContext;
        chartId: string;
        filingMonth: string;
    }): Promise<TaxRow[]>;
    workspaceCorrections(facts: {
        actor: WorkContext;
        chartId: string;
    }): Promise<TaxRow[]>;
    correctionDetailRows(facts: {
        id: string;
        actor: WorkContext;
        correction: TaxRow;
    }): Promise<TaxRow[]>;
    entryHistory(facts: {
        actor: WorkContext;
        entry: TaxRow;
    }): Promise<TaxRow[]>;
    currentEntry(facts: {
        actor: WorkContext;
        entry: TaxRow;
    }): Promise<TaxRow | null>;
    sourceVersion(facts: {
        entry: TaxRow;
        actor: WorkContext;
    }): Promise<TaxRow | null>;
    packageEvents(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<TaxRow[]>;
    registerJson(facts: {
        actor: WorkContext;
        p: TaxRow;
    }): Promise<string | null>;
    pendingCorrections(facts: {
        actor: WorkContext;
        p: TaxRow;
    }): Promise<number | null>;
    closeProfile(facts: {
        actor: WorkContext;
        endDate: string;
    }): Promise<TaxRow | null>;
}
export function createVatRegister(ports: VatRegisterPorts) {
    const { validateVatFacts } = createVatFactsPolicy(ports.error);
    function companyFinance(actor: WorkContext, permission: "finance.write" | "finance.approve") {
        actor = ports.permission(actor, permission);
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_FINANCE_REQUIRED", "Use a Company Finance assignment for the statutory register.");
        return actor;
    }
    async function vatSource(actor: WorkContext, sourceType: "invoice" | "commerce_return" | "billing_credit", id: string) {
        const source = await ports.vatSource(actor, sourceType, id);
        if (!source || !['posted', 'reversed'].includes(String(source.journal_status)))
            throw ports.error(404, "POSTED_TAX_SOURCE_REQUIRED", "A posted document and its immutable journal evidence are required.");
        if (source.currency !== "THB" || source.journal_currency !== "THB" || source.base_currency !== "THB")
            throw ports.error(422, "VAT_FX_ADAPTER_REQUIRED", "This register currently accepts THB documents in the approved THB statutory ledger.");
        if (!source.tax_account_id)
            throw ports.error(422, "TAX_ACCOUNT_REQUIRED", "The originating document must identify its posted VAT account.");
        const negative = sourceType !== "invoice", sign = negative ? -1 : 1, direction = source.kind === "receivable" ? "output" : "input";
        const evidence = JSON.parse(String(source.financial_evidence)) as {
            lines: Array<{
                account_id: string;
                base_debit_minor: number;
                base_credit_minor: number;
            }>;
        };
        const ledger = evidence.lines.filter(l => l.account_id === source.tax_account_id).reduce((n, l) => n + (direction === "output" ? Number(l.base_credit_minor) - Number(l.base_debit_minor) : Number(l.base_debit_minor) - Number(l.base_credit_minor)), 0);
        if (ledger !== sign * Number(source.tax_minor))
            throw ports.error(409, "TAX_SOURCE_GL_MISMATCH", "Document VAT does not agree with its posted tax-account lines. Correct the originating evidence first.");
        return { source, sourceHash: ports.digest(source), direction, sign, ledger };
    }
    async function profile(actor: WorkContext, chartId: string, date: string) {
        const row = await ports.profile(actor, chartId, date);
        if (!row)
            throw ports.error(409, "STATUTORY_PROFILE_REQUIRED", "Approve the Company's effective Thai statutory ledger and accounting policies first.");
        return row;
    }
    async function packageRow(actor: WorkContext, id: string) {
        const row = await ports.package(actor, id);
        if (!row)
            throw ports.error(404, "VAT_PACKAGE_NOT_FOUND", "VAT preparation package not found in this Company.");
        return row;
    }
    async function correctionRow(actor: WorkContext, id: string) {
        const row = await ports.correction(actor, id);
        if (!row)
            throw ports.error(404, "VAT_CORRECTION_NOT_FOUND", "VAT correction not found in this Company.");
        return row;
    }
    async function register(actor: WorkContext, chartId: string, filingMonth: string) {
        const json = String(await ports.fullRegister(actor, chartId, filingMonth));
        const entries = JSON.parse(json) as TaxRow[];
        if (entries.length > 1000)
            throw ports.error(422, "VAT_PACKAGE_LIMIT", "Split/review this register through a larger-volume adapter before preparing more than 1,000 records.");
        return { json, entries, sourceHash: ports.digest(JSON.parse(json)) };
    }
    async function vatReconciliation(actor: WorkContext, chartId: string, endDate: string, effectiveDate: string) {
        const rows = await ports.reconciliationRows(actor, chartId, effectiveDate, endDate);
        const missing = Number(await ports.missingSources(actor, chartId, effectiveDate, endDate) ?? 0);
        const pendingCorrections = Number(await ports.pendingCorrectionCount(actor, chartId, endDate) ?? 0);
        return { rows, missing, pendingCorrections, ready: missing === 0 && pendingCorrections === 0 && rows.every(r => Number(r.ledger_minor) === Number(r.register_minor)) };
    }
    async function prepareVatEntry(actor: WorkContext, raw: z.infer<typeof vatEntrySchema>, key: string) {
        actor = companyFinance(actor, "finance.write");
        const input = vatEntrySchema.parse(raw), id = `tax-entry-${key}`;
        await ports.ready(actor);
        const replay = await ports.replay<VatEntryResult>(actor, key, "finance.tax.prepare", id, input);
        if (replay)
            return replay;
        const current = await vatSource(actor, input.sourceType, input.sourceId), { source, direction, ledger } = current;
        if (Number(source.version) !== input.expectedSourceVersion)
            throw ports.error(409, "VERSION_CONFLICT", "The originating document changed. Review its latest evidence.");
        await profile(actor, String(source.chart_id), String(source.accounting_date));
        const original = input.sourceType !== "invoice" ? await ports.originalVatEntry(actor, source) : null;
        validateVatFacts(input, direction, ledger, source, original);
        const taxEntry = createVatTaxEntryPlan(id, input, current, original, ports.now()), deductible = taxEntry.deductibleMinor;
        const familyId = String(input.sourceType === "invoice" ? source.id : source.invoice_id);
        return ports.commitPrepareVatEntry({ actor, key, input, id, ledger, deductible, source, familyId, taxEntry });
    }
    async function reviewVatEntry(actor: WorkContext, id: string, action: "approve" | "reject", raw: z.infer<typeof taxReviewSchema>, key: string) {
        actor = companyFinance(ports.permission(actor, "finance.write"), "finance.approve");
        const input = taxReviewSchema.parse(raw), operation = `finance.tax.${action}`;
        await ports.ready(actor);
        const replay = await ports.replay<VatReviewResult>(actor, key, operation, id, input);
        if (replay)
            return replay;
        const entry = await ports.entry({ id, actor });
        if (!entry)
            throw ports.error(404, "TAX_ENTRY_NOT_FOUND", "Tax entry not found in this Company.");
        if (entry.correction_id)
            throw ports.error(409, "VAT_CORRECTION_WORKFLOW_REQUIRED", "Review the complete linked correction together; individual replacement review is unavailable.");
        if (entry.status !== "prepared" || entry.version !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Refresh the tax entry before reviewing it.");
        if (samePerson(actor, entry.prepared_by))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different Finance person must review tax facts and source evidence.");
        let verifiedSource: VatSource | null = null;
        if (action === "approve") {
            const current = await vatSource(actor, entry.source_type as "invoice" | "commerce_return" | "billing_credit", String(entry.source_id));
            if (current.sourceHash !== entry.source_hash)
                throw ports.error(409, "TAX_SOURCE_CHANGED", "The source changed after preparation. Reject this draft and prepare the current source.");
            verifiedSource = current;
        }
        const status = action === "approve" ? "approved" : "rejected";
        return ports.commitReviewVatEntry({ actor, key, operation, input, id, status, verifiedSource, entry });
    }
    async function prepareVatCorrection(actor: WorkContext, entryId: string, raw: z.infer<typeof vatCorrectionSchema>, key: string) {
        actor = companyFinance(actor, "finance.write");
        const input = vatCorrectionSchema.parse(raw), id = `vat-correction-${key}`, operation = "finance.vat-correction.prepare";
        await ports.ready(actor);
        const replay = await ports.replay<VatCorrectionResult>(actor, key, operation, id, { entryId, ...input });
        if (replay)
            return replay;
        const previous = await ports.previousEntry({ entryId, actor });
        if (!previous)
            throw ports.error(404, "CURRENT_APPROVED_TAX_ENTRY_REQUIRED", "Select the latest approved entry in this Company.");
        if (Number(previous.version) !== input.expectedEntryVersion)
            throw ports.error(409, "VERSION_CONFLICT", "The reviewed entry changed. Refresh its current revision.");
        const current = await vatSource(actor, previous.source_type as "invoice" | "commerce_return" | "billing_credit", String(previous.source_id));
        if (Number(current.source.version) !== input.expectedSourceVersion)
            throw ports.error(409, "VERSION_CONFLICT", "The originating document changed. Review its latest evidence.");
        await profile(actor, String(previous.chart_id), String(previous.accounting_date));
        const familyId = String(previous.source_type === "invoice" ? previous.source_id : current.source.invoice_id), json = await ports.familyEvidence(actor, familyId), family = JSON.parse(json) as {
            entries: TaxRow[];
            returns: Array<{
                id: string;
                status: string;
            }>;
            billingCredits: Array<{
                id: string;
                status: string;
            }>;
        };
        if (await ports.pendingFamily({ actor, familyId }))
            throw ports.error(409, "VAT_CORRECTION_PENDING", "Review or reject the pending correction for this document family first.");
        if (family.billingCredits.some(r => r.status === 'posted' && !family.entries.some(e => e.source_type === 'billing_credit' && e.source_id === r.id)) || family.entries.some(e => e.status === "prepared") || family.returns.some(r => r.status === "posted" && !family.entries.some(e => e.source_type === "commerce_return" && e.source_id === r.id)))
            throw ports.error(409, "VAT_FAMILY_REVIEW_REQUIRED", "Review every posted invoice/return VAT entry before correcting this document family.");
        if (family.returns.length > 1000)
            throw ports.error(422, "VAT_CORRECTION_LIMIT", "This document family requires a larger-volume correction adapter.");
        const related = previous.source_type === "invoice" ? family.entries.filter(e => e.source_type !== "invoice") : [];
        if (related.length >= 100)
            throw ports.error(422, "VAT_CORRECTION_LIMIT", "A correction may replace at most 100 linked VAT entries together.");
        const facts = vatEntrySchema.parse({ ...input, sourceType: previous.source_type, sourceId: previous.source_id }), original = previous.source_type !== "invoice" ? await ports.originalVatEntry(actor, current.source) : null;
        validateVatFacts(facts, current.direction, current.ledger, current.source, original);
        const members = [{ previous, current, facts, original, id: `${id}-entry-0` }];
        const replacementOriginal: TaxRow = { ...previous, id: members[0].id, tax_point_date: facts.taxPointDate, claim_month: facts.claimMonth, document_reference: facts.documentReference, treatment: facts.treatment, deductible_bps: facts.deductibleBps };
        for (const old of related) {
            const source = await vatSource(actor, old.source_type as "commerce_return" | "billing_credit", String(old.source_id)), oldFacts = JSON.parse(String(old.evidence_json)).facts;
            const newFacts = vatEntrySchema.parse({ ...oldFacts, sourceType: old.source_type, sourceId: old.source_id, expectedSourceVersion: Number(source.source.version), treatment: facts.treatment, deductibleBps: facts.deductibleBps, legalBasis: facts.legalBasis });
            validateVatFacts(newFacts, source.direction, source.ledger, source.source, replacementOriginal);
            members.push({ previous: old, current: source, facts: newFacts, original: replacementOriginal, id: `${id}-entry-${members.length}` });
        }
        const taxEntries = members.map(member => createVatTaxEntryPlan(member.id, member.facts, member.current, member.original, ports.now(), { id, replacesId: String(member.previous.id), reason: input.reason }));
        return ports.commitPrepareVatCorrection({ actor, key, operation, entryId, input, id, members, taxEntries, familyId, json, previous });
    }
    async function reviewVatCorrection(actor: WorkContext, id: string, action: "approve" | "reject", raw: z.infer<typeof taxReviewSchema>, key: string) {
        actor = companyFinance(ports.permission(actor, "finance.write"), "finance.approve");
        const input = taxReviewSchema.parse(raw), operation = `finance.vat-correction.${action}`;
        await ports.ready(actor);
        const replay = await ports.replay<VatCorrectionResult>(actor, key, operation, id, input);
        if (replay)
            return replay;
        const correction = await correctionRow(actor, id);
        if (correction.status !== "prepared" || Number(correction.version) !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Refresh the correction before reviewing it.");
        if (samePerson(actor, correction.prepared_by))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different Finance person must review the complete linked correction.");
        const entries = await ports.replacementEntries({ id, actor });
        if (!entries.length || entries.length > 100 || entries.some(e => e.status !== "prepared"))
            throw ports.error(409, "VAT_CORRECTION_INCOMPLETE", "The correction's replacement entries are incomplete.");
        let verifiedFamilyJson: string | null = null;
        const verifiedSources: VatSource[] = [];
        if (action === "approve") {
            const json = await ports.familyEvidence(actor, String(correction.family_source_id));
            if (json !== correction.family_evidence_json)
                throw ports.error(409, "VAT_CORRECTION_STALE", "The source family changed. Reject this correction and prepare the current evidence.");
            verifiedFamilyJson = json;
            for (const entry of entries) {
                const current = await vatSource(actor, entry.source_type as "invoice" | "commerce_return" | "billing_credit", String(entry.source_id));
                if (current.sourceHash !== entry.source_hash)
                    throw ports.error(409, "TAX_SOURCE_CHANGED", "A source changed after preparation. Reject the complete correction and prepare it again.");
                verifiedSources.push(current);
            }
        }
        const status = action === "approve" ? "approved" : "rejected", now = ports.now();
        return ports.commitReviewVatCorrection({ actor, key, operation, input, id, status, entries, verifiedFamilyJson, correction, verifiedSources, now });
    }
    async function prepareVatPackage(actor: WorkContext, raw: z.infer<typeof vatPackageSchema>, key: string) {
        actor = companyFinance(actor, "finance.write");
        const input = vatPackageSchema.parse(raw), id = `vat-package-${key}`, operation = "finance.vat-package.prepare";
        await ports.ready(actor);
        const replay = await ports.replay<VatPackageResult>(actor, key, operation, id, input);
        if (replay)
            return replay;
        const dates = monthDates(input.filingMonth), p = await profile(actor, input.chartId, dates.start), r = await register(actor, input.chartId, input.filingMonth);
        if (r.entries.some(e => e.status !== "approved"))
            throw ports.error(409, "VAT_REVIEW_REQUIRED", "Review or reject every prepared entry in the filing month first.");
        const reconciliation = await vatReconciliation(actor, input.chartId, dates.end, String(p.effective_date));
        if (!reconciliation.ready)
            throw ports.error(409, "VAT_GL_RECONCILIATION_REQUIRED", "Review all source VAT records and resolve tax-account differences before preparing the return.");
        let original: TaxRow | null = null;
        if (input.filingKind === "additional") {
            if (!input.originalPackageId)
                throw ports.error(422, "ORIGINAL_PACKAGE_REQUIRED", "Select the previously filed original return.");
            original = await packageRow(actor, input.originalPackageId);
            if (!["filed", "paid"].includes(String(original.status)) || original.chart_id !== input.chartId || original.filing_month !== input.filingMonth || original.filing_kind !== "original")
                throw ports.error(409, "ORIGINAL_PACKAGE_NOT_FILED", "An additional return must reference the filed original for the same ledger and month.");
        }
        else if (input.originalPackageId)
            throw ports.error(422, "ORIGINAL_PACKAGE_INVALID", "An original return cannot reference another package.");
        const total = (direction: string, column: string) => r.entries.filter(e => e.direction === direction).reduce((n, e) => n + Number(e[column]), 0), outputVat = total("output", "tax_minor"), inputVat = total("input", "tax_minor"), deductibleInput = total("input", "deductible_minor"), net = outputVat - deductibleInput;
        const result = { outputVatMinor: outputVat, inputVatMinor: inputVat, deductibleInputMinor: deductibleInput, disallowedInputMinor: inputVat - deductibleInput, netMinor: net, payableMinor: Math.max(0, net), creditMinor: Math.max(0, -net) };
        const snapshot: VatSnapshot = { register: r.entries, reconciliation, profileId: p.id, result, priorFiledResult: original ? JSON.parse(String(original.snapshot_json)).result : null, preparationOnly: true, officialFormatValidated: false, priorCreditsIncluded: false };
        const runId = `vat-calc-${key}`, now = ports.now();
        return ports.commitPrepareVatPackage({ actor, key, operation, input, id, result, r, dates, p, reconciliation, runId, now, snapshot, original });
    }
    async function approveVatPackage(actor: WorkContext, id: string, raw: z.infer<typeof taxReviewSchema>, key: string) {
        actor = companyFinance(ports.permission(actor, "finance.write"), "finance.approve");
        const input = taxReviewSchema.parse(raw), operation = "finance.vat-package.approve";
        await ports.ready(actor);
        const replay = await ports.replay<VatReviewResult>(actor, key, operation, id, input);
        if (replay)
            return replay;
        const p = await packageRow(actor, id);
        if (p.status !== "prepared" || p.version !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Refresh the package before review.");
        if (samePerson(actor, p.prepared_by))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different Finance approver must review the VAT package.");
        const r = await register(actor, String(p.chart_id), String(p.filing_month));
        if (r.sourceHash !== p.source_hash)
            throw ports.error(409, "VAT_PACKAGE_STALE", "The register changed after preparation. Supersede this package and prepare a new version.");
        const snapshot = JSON.parse(String(p.snapshot_json)), currentProfile = await profile(actor, String(p.chart_id), monthDates(String(p.filing_month)).start);
        const current = await vatReconciliation(actor, String(p.chart_id), String(p.effective_date), String(currentProfile.effective_date));
        if (!current.ready || ports.digest(current) !== ports.digest(snapshot.reconciliation))
            throw ports.error(409, "VAT_PACKAGE_STALE", "The posted VAT evidence changed after preparation.");
        return ports.commitApproveVatPackage({ actor, key, operation, input, id, p, r, currentProfile, current });
    }
    async function exportVatWorkpaper(actor: WorkContext, id: string, raw: z.infer<typeof taxReviewSchema>, key: string) {
        actor = companyFinance(actor, "finance.write");
        const input = taxReviewSchema.parse(raw), operation = "finance.vat-package.export";
        await ports.ready(actor);
        const replay = await ports.replay<VatExportResult>(actor, key, operation, id, input);
        if (replay)
            return replay;
        const p = await packageRow(actor, id);
        if (p.status !== "approved_for_export" || p.version !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Only the current independently approved package can be exported.");
        const r = await register(actor, String(p.chart_id), String(p.filing_month));
        if (r.sourceHash !== p.source_hash)
            throw ports.error(409, "VAT_PACKAGE_STALE", "The register changed; prepare and review a replacement package.");
        // This is an ERPX workpaper, deliberately not labelled as an RD import file.
        const snapshot = JSON.parse(String(p.snapshot_json)), currentProfile = await profile(actor, String(p.chart_id), monthDates(String(p.filing_month)).start), current = await vatReconciliation(actor, String(p.chart_id), String(p.effective_date), String(currentProfile.effective_date));
        if (!current.ready || ports.digest(current) !== ports.digest(snapshot.reconciliation))
            throw ports.error(409, "VAT_PACKAGE_STALE", "The posted VAT evidence changed after approval.");
        // Prefix potentially executable spreadsheet cells; document references and
        // reviewer notes are untrusted text even inside a preparation CSV.
        const csv = (value: unknown) => { let text = String(value ?? ""); if (/^[\s]*[=+@-]/.test(text) && !/^-[\d]+(?:\.\d+)?$/.test(text))
            text = `'${text}`; return `"${text.replaceAll('"', '""')}"`; };
        const content = "\uFEFF" + [
            ["ERPX ภ.พ.30 กระดาษทำการเพื่อให้คนตรวจและกรอกยื่น", "THB", p.filing_month, "ไม่รวมเครดิตยกมา / ไม่ใช่ไฟล์นำเข้ากรมสรรพากร"],
            ["เอกสาร", "วันที่เอกสาร", "วันบัญชี", "วันเกิด VAT", "เดือนใช้สิทธิ", "ประเภท", "ฐานภาษี", "VAT", "ภาษีซื้อใช้สิทธิ", "เหตุผลตรวจ"],
            ...snapshot.register.map((e: TaxRow) => [e.document_reference, e.document_date, e.accounting_date, e.tax_point_date, e.claim_month, e.treatment, minorToDecimal(Number(e.taxable_minor)), minorToDecimal(Number(e.tax_minor)), e.direction === "input" ? minorToDecimal(Number(e.deductible_minor)) : "", e.review_reason]),
            ["ภาษีขาย", minorToDecimal(snapshot.result.outputVatMinor)], ["ภาษีซื้อใช้สิทธิ", minorToDecimal(snapshot.result.deductibleInputMinor)], ["ภาษีสุทธิก่อนเครดิตและปรับตามแบบ", minorToDecimal(snapshot.result.netMinor)],
        ].map(row => row.map(csv).join(",")).join("\r\n") + "\r\n", exportHash = ports.hashBytes(content);
        return ports.commitExportVatWorkpaper({ actor, key, operation, input, id, p, content, exportHash, r, currentProfile, current });
    }
    async function recordVatFiling(actor: WorkContext, id: string, raw: z.infer<typeof vatFilingSchema>, key: string) {
        actor = companyFinance(actor, "finance.write");
        const input = vatFilingSchema.parse(raw), operation = "finance.vat-package.file";
        await ports.ready(actor);
        const replay = await ports.replay<VatReviewResult>(actor, key, operation, id, input);
        if (replay)
            return replay;
        const p = await packageRow(actor, id);
        if (p.status !== "exported" || p.version !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Record a filing receipt against the exported package.");
        return ports.commitRecordVatFiling({ actor, key, operation, input, id });
    }
    async function supersedeVatPackage(actor: WorkContext, id: string, raw: z.infer<typeof taxReviewSchema>, key: string) {
        actor = companyFinance(actor, "finance.write");
        const input = taxReviewSchema.parse(raw), operation = "finance.vat-package.supersede";
        await ports.ready(actor);
        const replay = await ports.replay<VatReviewResult>(actor, key, operation, id, input);
        if (replay)
            return replay;
        const p = await packageRow(actor, id);
        if (!["prepared", "approved_for_export", "exported"].includes(String(p.status)) || p.version !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Only an unfiled package can be superseded. Filed returns require an additional return.");
        if (!samePerson(actor, p.prepared_by))
            actor = companyFinance(actor, "finance.approve");
        return ports.commitSupersedeVatPackage({ actor, key, operation, input, id });
    }
    async function vatWorkspace(actor: WorkContext, query: {
        chartId?: string;
        filingMonth?: string;
    } = {}) {
        actor = ports.permission(actor, "finance.read");
        await ports.ready(actor);
        const profiles = await ports.profiles({ actor });
        const chartId = query.chartId ?? String(profiles[0]?.statutory_chart_id ?? ""), filingMonth = month.parse(query.filingMonth ?? new Date(ports.now()).toISOString().slice(0, 7));
        if (chartId && !await ports.chart({ chartId, actor }))
            throw ports.error(404, "CHART_NOT_FOUND", "Ledger not found in this Company.");
        const dates = monthDates(filingMonth);
        const [entries, candidates, packages, corrections] = await Promise.all([
            ports.workspaceEntries({ actor, chartId, filingMonth }),
            ports.workspaceCandidates({ actor, chartId }),
            ports.workspacePackages({ actor, chartId, filingMonth }),
            ports.workspaceCorrections({ actor, chartId }),
        ]);
        const p = profiles.find(p => p.statutory_chart_id === chartId && String(p.effective_date) <= dates.start), reconciliation = p ? await vatReconciliation(actor, chartId, dates.end, String(p.effective_date)) : null;
        return { chartId, filingMonth, profiles, entries: entries, candidates: candidates, packages: packages, corrections: corrections, reconciliation };
    }
    async function vatCorrectionDetail(actor: WorkContext, id: string): Promise<VatCorrectionDetail> {
        actor = ports.permission(actor, "finance.read");
        await ports.ready(actor);
        const correction = await correctionRow(actor, id);
        const entries = await ports.correctionDetailRows({ id, actor, correction });
        return { correction, entries: entries.map(row => { const { previous_json, ...replacement } = row; return { previous: JSON.parse(String(previous_json)), replacement }; }) };
    }
    async function taxEntryDetail(actor: WorkContext, id: string) {
        actor = ports.permission(actor, "finance.read");
        await ports.ready(actor);
        const entry = await ports.entry({ id, actor });
        if (!entry)
            throw ports.error(404, "TAX_ENTRY_NOT_FOUND", "Tax entry not found in this Company.");
        const [history, current, source] = await Promise.all([
            ports.entryHistory({ actor, entry }),
            ports.currentEntry({ actor, entry }),
            ports.sourceVersion({ entry, actor }),
        ]);
        return { entry, history: history, currentId: current?.id ?? null, currentSourceVersion: source?.version ?? null };
    }
    async function vatPackageDetail(actor: WorkContext, id: string) {
        actor = ports.permission(actor, "finance.read");
        await ports.ready(actor);
        const p = await packageRow(actor, id);
        const [events, currentJson, pendingCorrections] = await Promise.all([
            ports.packageEvents({ id, actor }),
            ports.registerJson({ actor, p }),
            ports.pendingCorrections({ actor, p }),
        ]);
        return { package: p, snapshot: JSON.parse(String(p.snapshot_json)), events: events, currentRegisterMatches: ports.digest(JSON.parse(String(currentJson))) === p.source_hash, pendingCorrections: Number(pendingCorrections ?? 0) };
    }
    async function vatPackageExport(actor: WorkContext, id: string) {
        actor = ports.permission(actor, "finance.read");
        await ports.ready(actor);
        const p = await packageRow(actor, id);
        if (!p.export_content || !p.export_hash)
            throw ports.error(404, "VAT_EXPORT_NOT_FOUND", "This package has no retained export yet.");
        if (ports.hashBytes(String(p.export_content)) !== p.export_hash)
            throw ports.error(409, "VAT_EXPORT_INTEGRITY", "The retained export does not match its recorded hash.");
        return { content: String(p.export_content), exportHash: String(p.export_hash), filename: `ERPX-PP30-workpaper-${p.filing_month}.csv` };
    }
    async function calculateVatCloseEvidence(actor: WorkContext, endDate: string): Promise<VatCloseEvidence | null> {
        const p = await ports.closeProfile({ actor, endDate });
        if (!p)
            return null;
        const reconciliation = await vatReconciliation(actor, String(p.statutory_chart_id), endDate, String(p.effective_date));
        return { profileId: String(p.id), chartId: String(p.statutory_chart_id), effectiveDate: String(p.effective_date), endDate, ...reconciliation };
    }
    return { prepareVatEntry, reviewVatEntry, prepareVatCorrection, reviewVatCorrection, prepareVatPackage, approveVatPackage, exportVatWorkpaper, recordVatFiling, supersedeVatPackage, vatWorkspace, vatCorrectionDetail, taxEntryDetail, vatPackageDetail, vatPackageExport, calculateVatCloseEvidence, vatSource, vatReconciliation, register };
}
