type LegacyUnknown = {
    id: string;
    reason: string;
    ownerType?: string;
    ownerId?: string;
    kind?: string;
    targetCompanyId?: string;
};
type UnavailableSource = {
    id: string;
    companyId: string | null;
    ownerType: string;
    ownerId: string;
    purpose: string;
    mediaType: string;
    bytes: number;
    sourceState: string;
    scanAttempt: number | null;
    scanState: string | null;
    scanResult: string | null;
    scanError: string | null;
};
type StorageReconciliation = {
    unknownOwnership: number;
    unavailable: number;
    sealedMalware: number;
    unknownOwnershipSources?: string[];
    unavailableSources?: UnavailableSource[];
    crossCompanyLinks: string[];
    crossCompanyIntents: string[];
    crossCompanyStorage: string[];
    crossCompanyDerivatives: string[];
    crossCompanyPublications: string[];
    missingObjects: string[];
    sizeMismatches: string[];
    checksumMismatches: string[];
    missingLedger: string[];
    orphanObjects: string[];
    quotaDeltaBytes: number;
    duplicateLedgerObjects: number;
};
type HoldingResult = {
    holdingId: string;
    backfill: {
        unknown: LegacyUnknown[];
    };
    specialized: {
        unknown: LegacyUnknown[];
    };
    reconciliation: StorageReconciliation;
};
type BillingReconciliation = Pick<StorageReconciliation, "unavailable" | "sealedMalware" | "missingObjects" | "sizeMismatches" | "checksumMismatches" | "missingLedger" | "orphanObjects" | "quotaDeltaBytes" | "duplicateLedgerObjects">;
export type AttachmentReconciliationSnapshot = {
    apply: boolean;
    verifyChecksums: boolean;
    final: boolean;
    freezeConfirmed: boolean;
    gateReady: boolean;
    scanner: {
        enabled: boolean;
        ready: boolean;
        error: string | null;
    };
    preview: {
        ready: boolean;
        error: string | null;
    };
    billing: {
        ready: boolean;
    };
    billingReconciliation: {
        ready: boolean;
        report: BillingReconciliation;
    };
    results: HoldingResult[];
};
export type AttachmentAdjudicationBlocker = "policy_unknown" | "owner_unknown" | "format_outside_policy" | "specialized_source_unknown" | "company_scope_unassigned" | "unavailable_source" | "unavailable_inventory_incomplete" | "cross_company_link" | "cross_company_intent" | "cross_company_storage" | "cross_company_derivative" | "cross_company_publication" | "missing_object" | "size_mismatch" | "checksum_mismatch" | "missing_ledger" | "orphan_object" | "quota_delta" | "duplicate_ledger_object" | "sealed_malware_review" | "final_reconciliation_required" | "scanner_unavailable" | "preview_unavailable" | "billing_readiness_missing" | "gate_result_inconsistent";
export type AttachmentAdjudicationItem = {
    id: string;
    scope: {
        kind: "holding" | "billing" | "runtime";
        id: string;
    };
    subject: {
        kind: "attachment_source" | "legacy_record" | "storage_entry" | "storage_object" | "scope";
        id: string;
    };
    severity: "medium" | "high" | "critical";
    blocksGate: boolean;
    blockers: AttachmentAdjudicationBlocker[];
    findings: Array<{
        blocker: AttachmentAdjudicationBlocker;
        details: Record<string, unknown>;
    }>;
    requiredEvidence: string[];
    allowedResolutions: string[];
    decision: {
        status: "pending";
        resolution: null;
        evidenceReferences: [
        ];
        decidedBy: null;
        decidedAt: null;
        notes: null;
    };
};
export function createAttachmentAdjudication(hash: (value: string) => string) {
    const severityRank = { medium: 0, high: 1, critical: 2 } as const;
    const globalProhibitedShortcuts = [
        "delete or overwrite source bytes or audit rows",
        "relabel purpose or ownership without record-owner evidence",
        "mark a source clean without the configured scanner",
        "copy or link evidence across Companies",
        "manually set reconciliation receipts or the feature gate ready",
    ] as const;
    function storageReady(report: StorageReconciliation | BillingReconciliation) {
        const scoped = report as StorageReconciliation;
        return (scoped.unknownOwnership ?? 0) === 0 && report.unavailable === 0
            && (scoped.crossCompanyLinks?.length ?? 0) === 0 && (scoped.crossCompanyIntents?.length ?? 0) === 0 && (scoped.crossCompanyStorage?.length ?? 0) === 0
            && (scoped.crossCompanyDerivatives?.length ?? 0) === 0 && (scoped.crossCompanyPublications?.length ?? 0) === 0
            && report.missingObjects.length === 0 && report.sizeMismatches.length === 0 && report.checksumMismatches.length === 0
            && report.missingLedger.length === 0 && report.orphanObjects.length === 0 && report.quotaDeltaBytes === 0 && report.duplicateLedgerObjects === 0;
    }
    function holdingAttachmentReconciliationReady(result: HoldingResult) {
        return result.backfill.unknown.length === 0 && result.specialized.unknown.length === 0 && storageReady(result.reconciliation);
    }
    function allowedResolutions(blockers: AttachmentAdjudicationBlocker[]) {
        const values = new Set<string>();
        for (const blocker of blockers) {
            if (["policy_unknown", "owner_unknown", "format_outside_policy", "specialized_source_unknown"].includes(blocker)) {
                values.add("confirm canonical owner and an existing policy from source-record evidence");
                values.add("preserve quarantined while the owner decision remains pending");
                values.add("create a Legal-preservation byte copy only through the approved exception");
            }
            if (blocker === "company_scope_unassigned") {
                values.add("assign the canonical Company only after the live target record and Company scope are verified");
                values.add("preserve quarantined while Company scope evidence remains pending");
            }
            if (["unavailable_source", "unavailable_inventory_incomplete"].includes(blocker)) {
                values.add("repair scanner/runtime prerequisites and retry the immutable source");
                values.add("submit a valid replacement as a new source/revision");
                values.add("preserve sealed pending an approved security-destruction policy");
            }
            if (["missing_ledger", "quota_delta", "duplicate_ledger_object"].includes(blocker)) {
                values.add("append/reconcile accounting only after owner, object size and checksum are independently verified");
                values.add("preserve blocked until storage-accounting evidence is complete");
            }
            if (blocker === "orphan_object") {
                values.add("associate with a canonical source only after provenance and checksum proof");
                values.add("preserve outside the canonical quota pending an owner decision");
            }
            if (["missing_object", "size_mismatch", "checksum_mismatch"].includes(blocker)) {
                values.add("restore the verified original from an approved backup as a new controlled recovery event");
                values.add("preserve the database record blocked for incident review");
            }
            if (blocker.startsWith("cross_company_")) {
                values.add("isolate and investigate the malformed reference without granting access");
                values.add("recreate an authorized same-Company link only from independently verified provenance");
            }
            if (blocker === "sealed_malware_review")
                values.add("preserve sealed or use an approved security-destruction policy");
            if (["final_reconciliation_required", "scanner_unavailable", "preview_unavailable", "billing_readiness_missing", "gate_result_inconsistent"].includes(blocker))
                values.add("repair the prerequisite and repeat the frozen final reconciliation");
        }
        return [...values].sort();
    }
    function requiredEvidence(blockers: AttachmentAdjudicationBlocker[]) {
        const values = new Set<string>(["operator identity and timestamp", "source report digest"]);
        if (blockers.some(value => ["policy_unknown", "owner_unknown", "format_outside_policy", "specialized_source_unknown", "company_scope_unassigned"].includes(value)))
            values.add("record-owner confirmation and canonical source-record reference");
        if (blockers.some(value => ["unavailable_source", "missing_object", "size_mismatch", "checksum_mismatch", "missing_ledger", "orphan_object", "quota_delta", "duplicate_ledger_object"].includes(value)))
            values.add("object key, byte length and SHA-256 verification evidence");
        if (blockers.some(value => value.startsWith("cross_company_")))
            values.add("both Company scopes and authorization audit evidence");
        if (blockers.includes("unavailable_source"))
            values.add("latest scanner attempt, engine and signature timestamp");
        return [...values].sort();
    }
    function legacyReason(reason: string): AttachmentAdjudicationBlocker {
        if (reason === "POLICY_UNKNOWN")
            return "policy_unknown";
        if (reason === "OWNER_UNKNOWN")
            return "owner_unknown";
        if (reason === "FORMAT_OUTSIDE_POLICY")
            return "format_outside_policy";
        return "specialized_source_unknown";
    }
    function hashId(parts: string[]) { return `attachment-adjudication-${hash(parts.join("\0")).slice(0, 24)}`; }
    function buildAttachmentAdjudicationManifest(snapshot: AttachmentReconciliationSnapshot) {
        const mutable = new Map<string, Omit<AttachmentAdjudicationItem, "requiredEvidence" | "allowedResolutions" | "decision">>();
        const sourceKey = (scopeId: string, sourceId: string) => `holding:${scopeId}:attachment_source:${sourceId}`;
        const add = (scope: AttachmentAdjudicationItem["scope"], subject: AttachmentAdjudicationItem["subject"], blocker: AttachmentAdjudicationBlocker, severity: AttachmentAdjudicationItem["severity"], details: Record<string, unknown>, blocksGate = true) => {
            const key = `${scope.kind}:${scope.id}:${subject.kind}:${subject.id}`, existing = mutable.get(key);
            if (existing) {
                if (!existing.blockers.includes(blocker)) {
                    existing.blockers.push(blocker);
                    existing.findings.push({ blocker, details });
                }
                if (severityRank[severity] > severityRank[existing.severity])
                    existing.severity = severity;
                existing.blocksGate ||= blocksGate;
                return;
            }
            mutable.set(key, { id: hashId([scope.kind, scope.id, subject.kind, subject.id]), scope, subject, severity, blocksGate, blockers: [blocker], findings: [{ blocker, details }] });
        };
        for (const result of snapshot.results) {
            const scope = { kind: "holding" as const, id: result.holdingId }, knownSourceIds = new Set<string>();
            for (const unknown of result.backfill.unknown) {
                knownSourceIds.add(unknown.id);
                add(scope, { kind: "attachment_source", id: unknown.id }, legacyReason(unknown.reason), "high", { reason: unknown.reason, ownerType: unknown.ownerType ?? null, ownerId: unknown.ownerId ?? null, targetCompanyId: unknown.targetCompanyId ?? null });
            }
            for (const unknown of result.specialized.unknown)
                add(scope, { kind: "legacy_record", id: `${unknown.kind ?? "specialized"}:${unknown.id}` }, legacyReason(unknown.reason), "high", { reason: unknown.reason, kind: unknown.kind ?? null });
            for (const sourceId of result.reconciliation.unknownOwnershipSources ?? []) {
                knownSourceIds.add(sourceId);
                add(scope, { kind: "attachment_source", id: sourceId }, "company_scope_unassigned", "high", {});
            }
            for (const source of result.reconciliation.unavailableSources ?? []) {
                knownSourceIds.add(source.id);
                add(scope, { kind: "attachment_source", id: source.id }, "unavailable_source", "high", { companyId: source.companyId, ownerType: source.ownerType, ownerId: source.ownerId, purpose: source.purpose, mediaType: source.mediaType, bytes: source.bytes, sourceState: source.sourceState, scanAttempt: source.scanAttempt, scanState: source.scanState, scanResult: source.scanResult, scanError: source.scanError });
            }
            const unavailableDetailed = result.reconciliation.unavailableSources?.length ?? 0;
            if (unavailableDetailed !== result.reconciliation.unavailable)
                add(scope, { kind: "scope", id: "unavailable-source-inventory" }, "unavailable_inventory_incomplete", "high", { reported: result.reconciliation.unavailable, detailed: unavailableDetailed });
            if ((result.reconciliation.unknownOwnershipSources?.length ?? 0) !== result.reconciliation.unknownOwnership)
                add(scope, { kind: "scope", id: "company-scope-inventory" }, "company_scope_unassigned", "high", { reported: result.reconciliation.unknownOwnership, detailed: result.reconciliation.unknownOwnershipSources?.length ?? 0 });
            const crossCompany: [
                keyof StorageReconciliation,
                AttachmentAdjudicationBlocker
            ][] = [
                ["crossCompanyLinks", "cross_company_link"], ["crossCompanyIntents", "cross_company_intent"], ["crossCompanyStorage", "cross_company_storage"],
                ["crossCompanyDerivatives", "cross_company_derivative"], ["crossCompanyPublications", "cross_company_publication"],
            ];
            for (const [field, blocker] of crossCompany)
                for (const id of result.reconciliation[field] as string[])
                    add(scope, { kind: "legacy_record", id }, blocker, "critical", { field });
            for (const [field, blocker, severity] of [
                ["missingObjects", "missing_object", "critical"], ["sizeMismatches", "size_mismatch", "critical"], ["checksumMismatches", "checksum_mismatch", "critical"],
            ] as const)
                for (const id of result.reconciliation[field])
                    add(scope, { kind: "storage_entry", id }, blocker, severity, { field });
            for (const id of result.reconciliation.missingLedger) {
                const key = sourceKey(result.holdingId, id);
                if (knownSourceIds.has(id) && mutable.has(key))
                    add(scope, { kind: "attachment_source", id }, "missing_ledger", "medium", {});
                else
                    add(scope, { kind: "storage_entry", id }, "missing_ledger", "medium", {});
            }
            for (const objectKey of result.reconciliation.orphanObjects)
                add(scope, { kind: "storage_object", id: objectKey }, "orphan_object", "high", {});
            if (result.reconciliation.quotaDeltaBytes !== 0)
                add(scope, { kind: "scope", id: "quota" }, "quota_delta", "medium", { deltaBytes: result.reconciliation.quotaDeltaBytes });
            if (result.reconciliation.duplicateLedgerObjects !== 0)
                add(scope, { kind: "scope", id: "ledger-duplicates" }, "duplicate_ledger_object", "high", { count: result.reconciliation.duplicateLedgerObjects });
            if (result.reconciliation.sealedMalware !== 0)
                add(scope, { kind: "scope", id: "sealed-malware" }, "sealed_malware_review", "critical", { count: result.reconciliation.sealedMalware }, false);
        }
        const billingScope = { kind: "billing" as const, id: "control" }, billingReport = snapshot.billingReconciliation.report;
        if (billingReport.unavailable !== 0)
            add(billingScope, { kind: "scope", id: "unavailable-source-inventory" }, "unavailable_inventory_incomplete", "high", { reported: billingReport.unavailable, detailed: 0 });
        for (const [field, blocker, severity] of [
            ["missingObjects", "missing_object", "critical"], ["sizeMismatches", "size_mismatch", "critical"], ["checksumMismatches", "checksum_mismatch", "critical"], ["missingLedger", "missing_ledger", "medium"],
        ] as const)
            for (const id of billingReport[field])
                add(billingScope, { kind: "storage_entry", id }, blocker, severity, { field });
        for (const objectKey of billingReport.orphanObjects)
            add(billingScope, { kind: "storage_object", id: objectKey }, "orphan_object", "high", {});
        if (billingReport.quotaDeltaBytes !== 0)
            add(billingScope, { kind: "scope", id: "quota" }, "quota_delta", "medium", { deltaBytes: billingReport.quotaDeltaBytes });
        if (billingReport.duplicateLedgerObjects !== 0)
            add(billingScope, { kind: "scope", id: "ledger-duplicates" }, "duplicate_ledger_object", "high", { count: billingReport.duplicateLedgerObjects });
        if (billingReport.sealedMalware !== 0)
            add(billingScope, { kind: "scope", id: "sealed-malware" }, "sealed_malware_review", "critical", { count: billingReport.sealedMalware }, false);
        const runtimeScope = { kind: "runtime" as const, id: "global" };
        if (!snapshot.final || !snapshot.apply || !snapshot.verifyChecksums || !snapshot.freezeConfirmed)
            add(runtimeScope, { kind: "scope", id: "final-reconciliation" }, "final_reconciliation_required", "critical", { final: snapshot.final, apply: snapshot.apply, verifyChecksums: snapshot.verifyChecksums, freezeConfirmed: snapshot.freezeConfirmed });
        if (!snapshot.scanner.enabled || !snapshot.scanner.ready)
            add(runtimeScope, { kind: "scope", id: "scanner" }, "scanner_unavailable", "critical", { enabled: snapshot.scanner.enabled, ready: snapshot.scanner.ready, error: snapshot.scanner.error });
        if (!snapshot.preview.ready)
            add(runtimeScope, { kind: "scope", id: "preview" }, "preview_unavailable", "critical", { ready: snapshot.preview.ready, error: snapshot.preview.error });
        if (!snapshot.billing.ready && !storageReady(billingReport)) {
            // Concrete Billing findings above explain readiness; avoid a duplicate generic item.
        }
        else if (!snapshot.billing.ready || !snapshot.billingReconciliation.ready)
            add(runtimeScope, { kind: "scope", id: "billing-readiness" }, "billing_readiness_missing", "critical", { billingReady: snapshot.billing.ready, reconciliationReady: snapshot.billingReconciliation.ready });
        const computedGateReady = snapshot.final && snapshot.apply && snapshot.verifyChecksums && snapshot.freezeConfirmed && snapshot.scanner.enabled && snapshot.scanner.ready && snapshot.preview.ready
            && snapshot.billing.ready && snapshot.billingReconciliation.ready && snapshot.results.every(holdingAttachmentReconciliationReady);
        if (snapshot.gateReady !== computedGateReady)
            add(runtimeScope, { kind: "scope", id: "gate-result" }, "gate_result_inconsistent", "critical", { reported: snapshot.gateReady, computed: computedGateReady });
        const items: AttachmentAdjudicationItem[] = [...mutable.values()].map(item => {
            item.blockers.sort();
            item.findings.sort((a, b) => a.blocker.localeCompare(b.blocker));
            return { ...item, requiredEvidence: requiredEvidence(item.blockers), allowedResolutions: allowedResolutions(item.blockers), decision: { status: "pending", resolution: null, evidenceReferences: [], decidedBy: null, decidedAt: null, notes: null } };
        });
        items.sort((a, b) => `${a.scope.kind}:${a.scope.id}:${a.subject.kind}:${a.subject.id}`.localeCompare(`${b.scope.kind}:${b.scope.id}:${b.subject.kind}:${b.subject.id}`));
        const blockerCounts: Record<string, number> = {};
        for (const item of items)
            for (const blocker of item.blockers)
                blockerCounts[blocker] = (blockerCounts[blocker] ?? 0) + 1;
        return {
            schemaVersion: 1,
            sourceReportSha256: hash(JSON.stringify(snapshot)),
            readOnly: true,
            reportedGateReady: snapshot.gateReady,
            computedGateReady,
            summary: { items: items.length, blockingItems: items.filter(item => item.blocksGate).length, reviewOnlyItems: items.filter(item => !item.blocksGate).length, blockerCounts },
            prohibitedShortcuts: globalProhibitedShortcuts,
            items,
        };
    }
    return { holdingAttachmentReconciliationReady, buildAttachmentAdjudicationManifest };
}
