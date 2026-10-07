import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import { metricKey, definitionSchema, filtersSchema, emptyDefinition, defaultFilters, shareSchema, sharePreviewSchema, transferOwnershipSchema, type Filters, type ReportView, type ReportSummary, type MetricData, type MetricId, type Breakdown } from "@/server/modules/runtime/public/rules/canvas-domain";
import { executiveCoverage } from "@/lib/executive/coverage";
import { modulesForRole } from "@/lib/business-roles";
import type { WorkAssignment, WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { currentAuthority } from "@/server/modules/runtime/public/executive/access";
import type { canAuthor } from "@/server/modules/runtime/public/executive/access";
import type { requireAuthor } from "@/server/modules/runtime/public/executive/access";
import type { executiveEligible } from "@/server/modules/runtime/public/executive/access";
import type { metricBreakdowns } from "@/server/modules/runtime/public/executive/metrics";
import type { metricCatalogFor } from "@/server/modules/runtime/public/executive/metrics";
import type { metricSupportsComparison } from "@/server/modules/runtime/public/executive/metrics";
import type { metricUnavailable } from "@/server/modules/runtime/public/executive/metrics";
import type { htmlWidgetDocument } from "@/server/modules/runtime/public/executive/html-widget";
import type { loadIamAuthority } from "@/server/modules/identity_access/public/iam-store";
import type { iamRoles } from "@/server/modules/identity_access/public/iam-store";
import type { betaAccess } from "@/server/modules/identity_access/public/beta-access";
import { type ReportRow, type ShareRow, missing, shareView, saveSchema, previousPeriod, type PreviewAssignmentRow, type PreviewAssignment } from '../../domain/executive/reports';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../../shared';
import type { OwnerCommands } from '../../../workflow/public/owner-command-contract';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortValue<Value, Effect> = Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends {
    guard: (...args: infer Args) => unknown;
} ? Omit<Value, 'guard'> & {
    guard(...args: Args): Effect;
} : Value extends readonly {
    bind(...args: never[]): unknown;
    run(): unknown;
}[] ? Effect[] : Value;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortValue<Result, Effect>> : PortValue<Value, Effect>;
export interface ExecutiveReportRepository<Effect> {
    readReportAccessR(id: string, companyId: string): Promise<ReportRow | null>;
    readReportAccessShare(id: string, companyId: string, userId: string, now: number): Promise<ShareRow | null>;
    readReportAccessEffect(companyId: string, userId: string): Promise<Record<string, unknown> | null>;
    readReportViewShares(id: string): Promise<{
        results: (ShareRow)[];
    }>;
    readReportViewRevisions(id: string): Promise<{
        results: ({
            version: number;
            created_at: number;
        })[];
    }>;
    readReportLibraryRows(userId: string, now: number, companyId: string, userId2: string, value: 0 | 1): Promise<{
        results: (ReportRow & {
            shared_definition: string | null;
        })[];
    }>;
    readReportLibraryAccessSetup(now: number, now2: number, now3: number, companyId: string): Promise<{
        results: ({
            id: string;
            name: string;
            level: number;
            ready: boolean;
        })[];
    }>;
    stageCreateReportEffect(id: string, companyId: string, userId: string, value: string, value2: string, now: number, now2: number): Effect;
    stageCreateReportEffect2(id: string, value: string, value2: string, userId: string, now: number): Effect;
    stageSaveReportEffect(id: string): Effect;
    guard11(key: string, value: "report-version", id: string, expectedVersion: number): Effect;
    guard12(key: string, value: "report-share", id: string, version: number, now: number): Effect;
    stageSaveReportEffect2(value: string, value2: string, version: number, now: number, id: string): Effect;
    stageSaveReportEffect3(id: string, version: number, value: string, value2: string, userId: string, now: number): Effect;
    stageSaveReportEffect4(value: string, id: string): Effect;
    readRestoreReportR(id: string, version: number): Promise<{
        definition_json: string;
        filters_json: string;
    } | null>;
    readRecipientsEffect(companyId: string, userId: string, q: string, now: number, now2: number, now3: number, now4: number): Promise<{
        results: ({
            id: string;
            name: string;
        })[];
    }>;
    readActiveShareRecipientEffect(companyId: string, recipientId: string, now: number, now2: number, now3: number, now4: number): Promise<{
        legal_name: string;
    } | null>;
    stageSaveShareEffect(id: string): Effect;
    guard20(key: string, value: "share-report-version", id: string, userId: string, version: number): Effect;
    guard21(key: string, value: "share-recipient", recipientId: string, companyId: string, now: number, companyId2: string, recipientId2: string, now2: number, now3: number, now4: number): Effect;
    stageSaveShareEffect2(shareId: string, id: string, companyId: string, recipientId: string, legal_name: string, role: "viewer" | "editor", mode: "recipient" | "distribution", value: string, value2: string, value3: string, value4: string, expiresAt: number | null, now: number): Effect;
    readRecipientPreviewAssignmentsEffect(companyId: string, recipientId: string, now: number): Promise<Record<string, unknown> | null>;
    readRecipientPreviewAssignmentsRows(companyId: string, recipientId: string, now: number, now2: number, now3: number, now4: number): Promise<{
        results: (PreviewAssignmentRow)[];
    }>;
    readRecipientPreviewAssignmentsCurrentRows(companyId: string, recipientId: string, now: number): Promise<{
        results: ({
            assignment_id: string;
        })[];
    }>;
    readRecipientPreviewContextActiveContext(companyId: string, recipientId: string, assignmentId: string, now: number): Promise<{
        session_id: string;
        revision: number;
        assignment_version: number;
    } | null>;
    readRecipientPreviewContextRecipient(companyId: string, recipientId: string): Promise<{
        legal_name: string;
    } | null>;
    readOwnershipCandidatesRows(companyId: string, userId: string, now: number, now2: number, now3: number, now4: number): Promise<{
        results: (PreviewAssignmentRow & {
            user_id: string;
            legal_name: string;
        })[];
    }>;
    readOwnershipCandidatesActiveContexts(companyId: string, now: number, value: string[], rows: (PreviewAssignmentRow & {
        user_id: string;
        legal_name: string;
    })[]): Promise<{
        results: ({
            assignment_id: string;
        })[];
    }>;
    readOwnershipCandidatesShares(id: string): Promise<{
        results: ({
            id: string;
            version: number;
        })[];
    }>;
    stageTransferReportOwnershipEffect(id: string): Effect;
    guard32(key: string, value: "transfer-report-version", id: string, userId: string, expectedVersion: number): Effect;
    guard33(key: string, value: "transfer-target", id: string, companyId: string, userId: string, role: string, value2: number, assignmentScope: import("@/server/modules/identity_access/public/authorization").AssignmentScopeKind, scopeNodeId: string | null, now: number, now2: number, now3: number, now4: number, value3: number, value4: number, assignmentScope2: import("@/server/modules/identity_access/public/authorization").AssignmentScopeKind): Effect;
    guard34(key: string, value: "transfer-share-set", id: string, value2: string): Effect;
    stageTransferReportOwnershipEffect2(ownerId: string, version: number, now: number, id: string): Effect;
    stageTransferReportOwnershipEffect3(id: string, version: number, definition_json: string, filters_json: string, userId: string, now: number): Effect;
    stageTransferReportOwnershipEffect4(now: number, id: string): Effect;
    stageTransferReportOwnershipEffect5(now: number, id: string): Effect;
    stageRevokeShareEffect(id: string): Effect;
    guard40(key: string, value: "revoke-report-version", id: string, userId: string, version: number): Effect;
    stageRevokeShareEffect2(now: number, shareId: string, id: string): Effect;
}
export interface ExecutiveReportPorts<Effect> {
    repository: ExecutiveReportRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OwnerCommands<Effect>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): PortResult<ReturnType<typeof currentAuthority>, Effect>;
    canAuthor(...args: Tail<Parameters<typeof canAuthor>>): PortResult<ReturnType<typeof canAuthor>, Effect>;
    requireAuthor(...args: Tail<Parameters<typeof requireAuthor>>): PortResult<ReturnType<typeof requireAuthor>, Effect>;
    executiveEligible(...args: Tail<Parameters<typeof executiveEligible>>): PortResult<ReturnType<typeof executiveEligible>, Effect>;
    metricBreakdowns(...args: Parameters<typeof metricBreakdowns>): PortResult<ReturnType<typeof metricBreakdowns>, Effect>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): PortResult<ReturnType<typeof metricCatalogFor>, Effect>;
    metricSupportsComparison(...args: Parameters<typeof metricSupportsComparison>): PortResult<ReturnType<typeof metricSupportsComparison>, Effect>;
    metricUnavailable(...args: Parameters<typeof metricUnavailable>): PortResult<ReturnType<typeof metricUnavailable>, Effect>;
    queryMetric(actor: WorkContext, metric: MetricId, filters: Filters, breakdown?: Breakdown): Promise<MetricData>;
    htmlWidgetDocument(...args: Parameters<typeof htmlWidgetDocument>): PortResult<ReturnType<typeof htmlWidgetDocument>, Effect>;
    loadIamAuthority(...args: Tail<Parameters<typeof loadIamAuthority>>): PortResult<ReturnType<typeof loadIamAuthority>, Effect>;
    iamRoles(...args: Tail<Parameters<typeof iamRoles>>): PortResult<ReturnType<typeof iamRoles>, Effect>;
    betaAccess(...args: Tail<Parameters<typeof betaAccess>>): PortResult<ReturnType<typeof betaAccess>, Effect>;
}
export function createExecutiveReportApplication<Effect>(ports: ExecutiveReportPorts<Effect>) {
    const repository = ports.repository;
    async function reportAccess(a: WorkContext, id: string) {
        await ports.currentAuthority(a);
        const r = await repository.readReportAccessR(id, a.companyId);
        if (!r)
            throw missing();
        const owner = r.owner_id === a.userId;
        const share = owner ? null : await repository.readReportAccessShare(id, a.companyId, a.userId, ports.clock.now());
        if (owner)
            await ports.requireAuthor(a);
        else if (!share)
            throw missing();
        if (share && !await repository.readReportAccessEffect(a.companyId, a.userId))
            throw missing();
        const definition = definitionSchema.parse(JSON.parse(share?.definition_json ?? r.definition_json));
        const filters = filtersSchema.parse(JSON.parse(share?.filters_json ?? r.filters_json));
        const canEdit = owner ? hasPermission(a, "executive.report.edit") : share?.role === "editor" && await ports.canAuthor(a) && hasPermission(a, "executive.report.edit");
        return { r, share, owner, definition, filters, canEdit: !!canEdit };
    }
    async function reportView(a: WorkContext, id: string): Promise<ReportView> {
        const x = await reportAccess(a, id);
        const shares = x.owner ? (await repository.readReportViewShares(id)).results.map(shareView) : [];
        const revisions = x.owner ? (await repository.readReportViewRevisions(id)).results.map(r => ({ version: r.version, createdAt: r.created_at })) : [];
        await ports.currentAuthority(a);
        return { id, definitionVersion: 1, owner: x.owner, canEdit: x.canEdit, canShare: x.owner && hasPermission(a, "executive.report.share"), version: x.r.version, definition: x.definition, filters: x.filters, lockedFilters: !!x.share, updatedAt: x.r.updated_at, shares, revisions };
    }
    async function reportLibrary(a: WorkContext) {
        await ports.currentAuthority(a);
        const author = await ports.canAuthor(a);
        const rows = (await repository.readReportLibraryRows(a.userId, ports.clock.now(), a.companyId, a.userId, author ? 1 : 0)).results;
        const reports: ReportSummary[] = rows.map(r => { const d = definitionSchema.parse(JSON.parse(r.owner_id === a.userId ? r.definition_json : r.shared_definition!)); return { id: r.id, title: d.title, description: d.description, theme: d.theme, version: r.version, updatedAt: r.updated_at, owner: r.owner_id === a.userId, shared: !!r.shared_definition, widgets: d.widgets.length }; });
        const accessSetup = hasPermission(a, "iam.manage") && hasPermission(a, "people.read") && !a.branchId && !a.departmentId ? (await repository.readReportLibraryAccessSetup(ports.clock.now(), ports.clock.now(), ports.clock.now(), a.companyId)).results : [];
        return { canCreate: author, reports, catalog: await ports.metricCatalogFor(a), coverage: executiveCoverage(), eligible: await ports.executiveEligible(a), accessSetup };
    }
    async function createReport(a: WorkContext, key: string, title = "Untitled report") {
        await ports.requireAuthor(a);
        const definition = emptyDefinition(title), filters = defaultFilters(new Date(ports.clock.now())), id = `report-${key}`, now = ports.clock.now();
        return ports.commands.commit(a, key, "executive.report.create", { title }, id, 1, { id }, [
            repository.stageCreateReportEffect(id, a.companyId, a.userId, JSON.stringify(definition), JSON.stringify(filters), now, now),
            repository.stageCreateReportEffect2(id, JSON.stringify(definition), JSON.stringify(filters), a.userId, now),
        ]);
    }
    async function saveReport(a: WorkContext, id: string, input: z.infer<typeof saveSchema>, key: string) {
        const x = await reportAccess(a, id);
        if (!x.canEdit)
            throw new BusinessRuleError(403, "REPORT_READ_ONLY", "This report is shared as view only.");
        const replay = await ports.commands.replay<{
            id: string;
            version: number;
        }>(a, key, "executive.report.save", id, input);
        if (replay)
            return replay;
        if (!x.owner) {
            // Editors never gain the owner's hidden widgets or broaden an approved projection.
            if (JSON.stringify(definitionSchema.parse(JSON.parse(x.r.definition_json))) !== JSON.stringify(x.definition))
                throw new BusinessRuleError(409, "SHARED_REVISION_CHANGED", "The owner has a newer design. Ask them to publish the current revision before editing.");
            const published = x.definition.widgets.map(w => w.id).sort().join(",");
            const current = definitionSchema.parse(JSON.parse(x.r.definition_json)).widgets.map(w => w.id).sort().join(",");
            if (published !== current || input.definition.widgets.map(w => w.id).sort().join(",") !== published || JSON.stringify(input.filters) !== JSON.stringify(x.filters) || input.definition.widgets.some(w => !x.definition.widgets.some(p => p.id === w.id && metricKey(p) === metricKey(w))))
                throw new BusinessRuleError(403, "SHARE_BOUNDS", "Only the explicitly shared widgets can be edited. Ask the owner to add new data or change the approved filters.");
        }
        const allowed = (await ports.metricCatalogFor(a)).filter(m => m.available).map(m => m.id);
        if (input.definition.widgets.some(w => !allowed.includes(w.metric) || !ports.metricBreakdowns(w.metric).includes(w.breakdown ?? "default") || !!w.comparePeriod && !ports.metricSupportsComparison(w.metric)))
            throw new BusinessRuleError(403, "METRIC_FORBIDDEN", "A widget uses a metric or comparison unavailable to your selected role.");
        const version = input.expectedVersion + 1, now = ports.clock.now();
        return ports.commands.commit(a, key, "executive.report.save", input, id, version, { id, version }, [
            repository.stageSaveReportEffect(id),
            repository.guard11(key, "report-version", id, input.expectedVersion),
            ...(!x.owner ? [repository.guard12(key, "report-share", x.share!.id, x.share!.version, now)] : []),
            repository.stageSaveReportEffect2(JSON.stringify(input.definition), JSON.stringify(input.filters), version, now, id),
            repository.stageSaveReportEffect3(id, version, JSON.stringify(input.definition), JSON.stringify(input.filters), a.userId, now),
            // An editor can continue their bounded design edits. Other recipients stay pinned.
            ...(!x.owner ? [repository.stageSaveReportEffect4(JSON.stringify(input.definition), x.share!.id)] : []),
        ]);
    }
    async function restoreReport(a: WorkContext, id: string, version: number, expectedVersion: number, key: string) {
        const x = await reportAccess(a, id);
        if (!x.owner)
            throw missing();
        const r = await repository.readRestoreReportR(id, version);
        if (!r)
            throw missing();
        return saveReport(a, id, saveSchema.parse({ expectedVersion, definition: JSON.parse(r.definition_json), filters: JSON.parse(r.filters_json) }), key);
    }
    async function recipients(a: WorkContext, id: string, q: string) {
        const x = await reportAccess(a, id);
        if (!x.owner || !hasPermission(a, "executive.report.share"))
            throw missing();
        return (await repository.readRecipientsEffect(a.companyId, a.userId, q, ports.clock.now(), ports.clock.now(), ports.clock.now(), ports.clock.now())).results;
    }
    async function activeShareRecipient(a: WorkContext, recipientId: string) {
        const now = ports.clock.now();
        return repository.readActiveShareRecipientEffect(a.companyId, recipientId, now, now, now, now);
    }
    async function validateShareCandidate(a: WorkContext, id: string, input: z.infer<typeof shareSchema>, validateDistributionReads = true) {
        await ports.requireAuthor(a, "executive.report.share");
        const x = await reportAccess(a, id);
        if (!x.owner)
            throw missing();
        const person = await activeShareRecipient(a, input.recipientId);
        const target = { ...a, userId: input.recipientId, role: "employee", iam: undefined };
        if (!person || input.recipientId === a.userId)
            throw new BusinessRuleError(422, "RECIPIENT_INVALID", "Choose an active employee in this company.");
        if (input.role === "editor" && (!await ports.executiveEligible(target) || input.mode === "distribution"))
            throw new BusinessRuleError(422, "EDITOR_INELIGIBLE", "Editors must be eligible executives using their own source access.");
        if (input.expiresAt !== null && input.expiresAt <= ports.clock.now())
            throw new BusinessRuleError(422, "SHARE_EXPIRED", "Choose a future expiry.");
        const widgets = x.definition.widgets.filter(w => input.widgetIds.includes(w.id));
        if (widgets.length !== new Set(input.widgetIds).size)
            throw new BusinessRuleError(422, "WIDGET_INVALID", "Choose widgets from the saved report.");
        if (input.role === "editor" && widgets.length !== x.definition.widgets.length)
            throw new BusinessRuleError(422, "EDITOR_SCOPE", "Editors need the complete report. Use viewer access for selected widgets.");
        if (input.mode === "distribution") {
            if (!hasPermission(a, "executive.data.distribute"))
                throw new BusinessRuleError(403, "DISTRIBUTION_FORBIDDEN", "Your role cannot distribute report data.");
            if (widgets.some(w => ["my-tasks", "projects", "overdue-project-milestones", "project-dependency-blockers", "project-actual-hours", "project-posted-cost", "routines", "approved-leave-duration", "authorized-overtime-duration", "open-operations-exceptions", "exception-resolution-time", "operations-escalations"].includes(w.metric)))
                throw new BusinessRuleError(422, "PERSONAL_METRIC", "Personal work metrics can only be shared using the recipient's own access.");
            if (widgets.some(w => ["available-stock", "inventory-shortages", "inventory-movements"].includes(w.metric)))
                throw new BusinessRuleError(422, "RECIPIENT_SOURCE_REQUIRED", "Inventory metrics can reveal product, warehouse, reorder threshold or source movement details; share them only when each recipient reads through their own Inventory access.");
            if (widgets.some(w => w.metric === "hotel-occupancy"))
                throw new BusinessRuleError(422, "RECIPIENT_SOURCE_REQUIRED", "Hotel occupancy aggregates remain recipient-scoped until aggregate disclosure and small-cohort rules are accepted.");
            if (widgets.some(w => w.metric === "active-support-incidents"))
                throw new BusinessRuleError(422, "RECIPIENT_SOURCE_REQUIRED", "Support incident counts remain recipient-scoped until aggregate disclosure and small-cohort rules are accepted.");
            if (widgets.some(w => ["active-subscriptions", "overdue-invoices"].includes(w.metric)))
                throw new BusinessRuleError(422, "RECIPIENT_SOURCE_REQUIRED", "Holding billing metrics are limited to Holding owners and cannot be distributed to report recipients.");
            if (widgets.some(w => w.metric === "payroll-summary"))
                throw new BusinessRuleError(422, "RECIPIENT_SOURCE_REQUIRED", "Payroll summaries must be read using each recipient's own Payroll permission.");
            if (validateDistributionReads)
                for (const widget of widgets) {
                    await ports.queryMetric(a, widget.metric, input.filters, widget.breakdown);
                    if (widget.comparePeriod)
                        await ports.queryMetric(a, widget.metric, previousPeriod(input.filters), widget.breakdown);
                }
        }
        const definition = { ...x.definition, widgets }, shareId = `share-${id}-${input.recipientId}`;
        return { x, person, widgets, definition, shareId };
    }
    async function saveShare(a: WorkContext, id: string, input: z.infer<typeof shareSchema>, key: string) {
        const { x, person, widgets, definition, shareId } = await validateShareCandidate(a, id, input);
        // Persist authority without browser/session credentials. Any authority revision change requires republishing.
        const publisher = { ...a, authority: undefined };
        return ports.commands.commit(a, key, "executive.report.share", input, id, x.r.version, { id: shareId }, [
            repository.stageSaveShareEffect(id),
            repository.guard20(key, "share-report-version", id, a.userId, x.r.version),
            repository.guard21(key, "share-recipient", input.recipientId, a.companyId, ports.clock.now(), a.companyId, input.recipientId, ports.clock.now(), ports.clock.now(), ports.clock.now()),
            repository.stageSaveShareEffect2(shareId, id, a.companyId, input.recipientId, person.legal_name, input.role, input.mode, JSON.stringify(widgets.map(w => w.id)), JSON.stringify(definition), JSON.stringify(input.filters), JSON.stringify(publisher), input.expiresAt, ports.clock.now()),
        ]);
    }
    async function recipientPreviewAssignments(a: WorkContext, id: string, recipientId: string): Promise<PreviewAssignment[]> {
        const x = await reportAccess(a, id);
        if (!x.owner || !hasPermission(a, "executive.report.share"))
            throw missing();
        if (!await repository.readRecipientPreviewAssignmentsEffect(a.companyId, recipientId, ports.clock.now()))
            return [];
        const rows = (await repository.readRecipientPreviewAssignmentsRows(a.companyId, recipientId, ports.clock.now(), ports.clock.now(), ports.clock.now(), ports.clock.now())).results;
        const roles = await ports.iamRoles(a.companyId, rows.map(row => row.role));
        const currentRows = (await repository.readRecipientPreviewAssignmentsCurrentRows(a.companyId, recipientId, ports.clock.now())).results;
        const currentIds = new Set(currentRows.map(row => row.assignment_id));
        return rows.flatMap(row => {
            const role = roles.get(row.role);
            if (!role || !role.scopes.includes(row.scope_kind))
                return [];
            const scope = row.scope_kind === "holding" ? "Company-wide" : row.scope_kind === "branch" ? row.scope_name ?? "Branch scope" : [row.scope_branch_name, row.scope_name].filter(Boolean).join(" · ") || "Department scope";
            return [{ row, roleName: role.name, label: `${role.name} · ${scope}`, scope, current: currentIds.has(row.id), permissions: role.permissions }];
        });
    }
    async function recipientPreviewContexts(a: WorkContext, id: string, recipientId: string) {
        const contexts = await recipientPreviewAssignments(a, id, recipientId);
        return contexts.map(context => ({ id: context.row.id, label: context.label, scope: context.scope, role: context.roleName, current: context.current }));
    }
    async function recipientPreviewContext(a: WorkContext, id: string, recipientId: string, assignmentId: string): Promise<WorkContext> {
        const candidates = await recipientPreviewAssignments(a, id, recipientId);
        const candidate = candidates.find(item => item.row.id === assignmentId);
        if (!candidate)
            throw new BusinessRuleError(422, "PREVIEW_CONTEXT_UNAVAILABLE", "Choose one of the recipient's currently active work roles.");
        const row = candidate.row;
        const scope = row.scope_kind === "holding"
            ? { kind: "holding" as const, holdingId: a.holdingId, holdingSlug: a.holdingSlug, holdingName: a.holdingName, companyId: a.companyId }
            : row.scope_kind === "branch"
                ? { kind: "branch" as const, holdingId: a.holdingId, holdingSlug: a.holdingSlug, holdingName: a.holdingName, companyId: a.companyId, branchId: row.scope_node_id!, branchSlug: row.scope_slug!, branchName: row.scope_name! }
                : { kind: "department" as const, holdingId: a.holdingId, holdingSlug: a.holdingSlug, holdingName: a.holdingName, companyId: a.companyId, branchId: row.scope_branch_id, branchSlug: row.scope_branch_slug ?? undefined, branchName: row.scope_branch_name ?? undefined, departmentId: row.scope_node_id!, departmentSlug: row.scope_slug!, departmentName: row.scope_name! };
        const branchId = scope.kind === "holding" ? null : scope.kind === "branch" ? scope.branchId : scope.branchId;
        const departmentId = scope.kind === "department" ? scope.departmentId : null;
        const assignment: WorkAssignment = { id: row.id, role: row.role, companyId: row.company_id, branchId, modules: JSON.parse(row.modules_json) as string[], scopeKind: row.scope_kind, scopeNodeId: row.scope_node_id, version: Number(row.version), validUntil: Number(row.valid_until), label: candidate.label };
        const activeContext = await repository.readRecipientPreviewContextActiveContext(a.companyId, recipientId, assignmentId, ports.clock.now());
        const loadedIam = await ports.loadIamAuthority(a.companyId, recipientId, row.role, activeContext?.session_id);
        const iam = { ...loadedIam, queuedAssignmentVersion: Number(row.version) };
        const beta = await ports.betaAccess(a.companyId, new Headers());
        const modules = beta ? [] : row.role.startsWith("custom_") ? [...new Set(iam.permissions.filter(permission => permission.startsWith("module.")).map(permission => permission.split(".")[1]))] : modulesForRole(row.role, assignment.modules);
        const recipient = await repository.readRecipientPreviewContextRecipient(a.companyId, recipientId);
        if (!recipient)
            throw new BusinessRuleError(422, "RECIPIENT_INVALID", "Choose an active employee in this company.");
        const context: WorkContext = { ...a, iam, userId: recipientId, name: recipient.legal_name, email: "", assignmentId: row.id, role: row.role, companyId: row.company_id, branchId, departmentId, scope, assignmentScope: row.scope_kind, inherited: false, beta, modules, assignments: [assignment], availableAssignments: [assignment], authority: activeContext ? { sessionId: activeContext.session_id, revision: Number(activeContext.revision), assignmentVersion: Number(activeContext.assignment_version) } : undefined, workspaces: [] };
        await ports.currentAuthority(context);
        return context;
    }
    async function previewShare(a: WorkContext, id: string, input: z.infer<typeof sharePreviewSchema>) {
        const candidate = await validateShareCandidate(a, id, shareSchema.parse({ recipientId: input.recipientId, mode: input.mode, role: "viewer", widgetIds: input.widgetIds, expiresAt: null, filters: input.filters }), false);
        const reader = input.mode === "distribution" ? a : await recipientPreviewContext(a, id, input.recipientId, input.assignmentId ?? "");
        const metrics: Record<string, MetricData> = {}, comparisons: Record<string, MetricData> = {};
        for (const widget of candidate.widgets) {
            const key = metricKey(widget);
            if (!metrics[key])
                try {
                    metrics[key] = await ports.queryMetric(reader, widget.metric, input.filters, widget.breakdown);
                }
                catch (error) {
                    const denied = error instanceof BusinessRuleError;
                    metrics[key] = ports.metricUnavailable(widget.metric, denied ? error.message : "This metric could not be previewed.", denied ? "unavailable" : "failed", input.filters.currency);
                }
            if (widget.comparePeriod)
                try {
                    if (!ports.metricSupportsComparison(widget.metric))
                        throw new BusinessRuleError(403, "COMPARISON_UNAVAILABLE", "This metric does not support period comparison.");
                    comparisons[widget.id] = await ports.queryMetric(reader, widget.metric, previousPeriod(input.filters), widget.breakdown);
                }
                catch (error) {
                    const denied = error instanceof BusinessRuleError;
                    comparisons[widget.id] = ports.metricUnavailable(widget.metric, denied ? error.message : "The comparison period could not be previewed.", denied ? "unavailable" : "failed", input.filters.currency);
                }
        }
        await ports.currentAuthority(a);
        if (reader !== a)
            await ports.currentAuthority(reader);
        if (reader === a && !await activeShareRecipient(a, input.recipientId))
            throw new BusinessRuleError(403, "RECIPIENT_ACCESS_CHANGED", "The recipient's active membership or work role changed. Refresh the share preview.");
        const fresh = await reportAccess(a, id);
        if (fresh.r.version !== candidate.x.r.version)
            throw new BusinessRuleError(409, "REPORT_CHANGED", "The report changed while its share preview was loading.");
        return { mode: input.mode, recipientContext: reader === a ? null : { assignmentId: reader.assignmentId, role: reader.role, scope: reader.scope.kind === "holding" ? "Company-wide" : reader.scope.kind === "branch" ? reader.scope.branchName : [reader.scope.branchName, reader.scope.departmentName].filter(Boolean).join(" · ") }, widgets: candidate.widgets.map(widget => ({ id: widget.id, title: widget.title, metric: widget.metric, data: metrics[metricKey(widget)], comparison: comparisons[widget.id] })), updatedAt: ports.clock.now() };
    }
    async function ownershipCandidates(a: WorkContext, id: string) {
        const x = await reportAccess(a, id);
        if (!x.owner || !hasPermission(a, "executive.report.share"))
            throw missing();
        const now = ports.clock.now(), rows = (await repository.readOwnershipCandidatesRows(a.companyId, a.userId, now, now, now, now)).results;
        const roles = await ports.iamRoles(a.companyId, rows.map(row => row.role));
        const activeContexts = rows.length ? (await repository.readOwnershipCandidatesActiveContexts(a.companyId, now, rows.map(row => row.id), rows)).results : [];
        const currentIds = new Set(activeContexts.map(row => row.assignment_id)), people = new Map<string, {
            id: string;
            name: string;
            contexts: {
                id: string;
                label: string;
                role: string;
                scope: string;
                current: boolean;
            }[];
        }>();
        for (const row of rows) {
            const role = roles.get(row.role);
            if (!role || !role.scopes.includes(row.scope_kind))
                continue;
            const scope = row.scope_kind === "holding" ? "Company-wide" : row.scope_kind === "branch" ? row.scope_name ?? "Branch scope" : [row.scope_branch_name, row.scope_name].filter(Boolean).join(" · ") || "Department scope";
            const person = people.get(row.user_id) ?? { id: row.user_id, name: row.legal_name, contexts: [] };
            person.contexts.push({ id: row.id, label: `${role.name} · ${scope}`, role: role.name, scope, current: currentIds.has(row.id) });
            people.set(row.user_id, person);
        }
        const shares = (await repository.readOwnershipCandidatesShares(id)).results;
        if (shares.length > 500)
            throw new BusinessRuleError(422, "TRANSFER_SHARE_LIMIT", "Revoke some shares before transferring this report.");
        return { people: [...people.values()], shares };
    }
    async function transferReportOwnership(a: WorkContext, id: string, input: z.infer<typeof transferOwnershipSchema>, key: string) {
        await ports.requireAuthor(a, "executive.report.share");
        const replay = await ports.commands.replay<{
            id: string;
            ownerId: string;
            version: number;
            revokedShares: number;
        }>(a, key, "executive.report.transfer", id, input);
        if (replay)
            return replay;
        const x = await reportAccess(a, id);
        if (!x.owner)
            throw missing();
        if (input.expectedVersion !== x.r.version)
            throw new BusinessRuleError(409, "REPORT_CHANGED", "The report changed. Reload it before transferring ownership.");
        const candidates = await ownershipCandidates(a, id), person = candidates.people.find(candidate => candidate.id === input.ownerId), context = person?.contexts.find(candidate => candidate.id === input.assignmentId);
        if (!person || !context)
            throw new BusinessRuleError(422, "TRANSFER_TARGET_INVALID", "Choose an active Executive with an eligible work role.");
        const target = await recipientPreviewContext(a, id, input.ownerId, input.assignmentId);
        await ports.requireAuthor(target, "executive.report.share");
        const expectedShares = [...input.expectedShares].sort((left, right) => left.id.localeCompare(right.id)), currentShares = [...candidates.shares].sort((left, right) => left.id.localeCompare(right.id));
        if (JSON.stringify(expectedShares) !== JSON.stringify(currentShares))
            throw new BusinessRuleError(409, "TRANSFER_SHARES_CHANGED", "Report shares changed. Review the latest transfer preview.");
        await ports.currentAuthority(target);
        const version = x.r.version + 1, now = ports.clock.now(), targetAssignment = target.assignments.find(assignment => assignment.id === target.assignmentId)!;
        return ports.commands.commit(a, key, "executive.report.transfer", input, id, version, { id, ownerId: input.ownerId, version, revokedShares: currentShares.length }, [
            repository.stageTransferReportOwnershipEffect(id),
            repository.guard32(key, "transfer-report-version", id, a.userId, input.expectedVersion),
            repository.guard33(key, "transfer-target", targetAssignment.id, target.companyId, target.userId, target.role, targetAssignment.version ?? 0, target.assignmentScope, targetAssignment.scopeNodeId, now, now, now, now, target.iam?.identityVersion ?? 0, target.iam?.roleRevision ?? 0, target.assignmentScope),
            repository.guard34(key, "transfer-share-set", id, JSON.stringify(currentShares)),
            repository.stageTransferReportOwnershipEffect2(input.ownerId, version, now, id),
            repository.stageTransferReportOwnershipEffect3(id, version, x.r.definition_json, x.r.filters_json, a.userId, now),
            repository.stageTransferReportOwnershipEffect4(now, id),
            repository.stageTransferReportOwnershipEffect5(now, id),
        ]);
    }
    async function revokeShare(a: WorkContext, id: string, shareId: string, key: string) {
        await ports.requireAuthor(a, "executive.report.share");
        const x = await reportAccess(a, id);
        if (!x.owner)
            throw missing();
        return ports.commands.commit(a, key, "executive.report.revoke", { shareId }, id, x.r.version, { id: shareId }, [
            repository.stageRevokeShareEffect(id),
            repository.guard40(key, "revoke-report-version", id, a.userId, x.r.version),
            repository.stageRevokeShareEffect2(ports.clock.now(), shareId, id),
        ]);
    }
    async function reportData(a: WorkContext, id: string, filters?: Filters) {
        const x = await reportAccess(a, id);
        let f = x.share ? x.filters : filters ?? x.filters;
        if (x.share && filters) {
            if (filters.currency === x.filters.currency && filters.from >= x.filters.from && filters.to <= x.filters.to)
                f = filters;
        }
        let reader = a;
        if (x.share?.mode === "distribution") {
            reader = JSON.parse(x.share.publisher_json) as WorkContext;
            await ports.requireAuthor(reader, "executive.data.distribute");
        }
        const metrics = [...new Map(x.definition.widgets.map(w => [metricKey(w), w])).values()];
        const results: Record<string, MetricData> = {};
        for (const widget of metrics)
            try {
                results[metricKey(widget)] = await ports.queryMetric(reader, widget.metric, f, widget.breakdown);
            }
            catch (e) {
                const denied = e instanceof BusinessRuleError;
                results[metricKey(widget)] = ports.metricUnavailable(widget.metric, denied ? e.message : "This metric could not be refreshed. Try again.", denied ? "unavailable" : "failed", f.currency);
            }
        const comparisons: Record<string, MetricData> = {};
        for (const widget of x.definition.widgets.filter(widget => widget.comparePeriod))
            try {
                if (!ports.metricSupportsComparison(widget.metric))
                    throw new BusinessRuleError(403, "COMPARISON_UNAVAILABLE", "This metric does not support period comparison.");
                comparisons[widget.id] = await ports.queryMetric(reader, widget.metric, previousPeriod(f), widget.breakdown);
            }
            catch (e) {
                const denied = e instanceof BusinessRuleError;
                comparisons[widget.id] = ports.metricUnavailable(widget.metric, denied ? e.message : "The comparison period could not be refreshed. Try again.", denied ? "unavailable" : "failed", f.currency);
            }
        // Recheck after reads, including publisher and share version, before returning any payload.
        await ports.currentAuthority(a);
        if (reader !== a)
            await ports.requireAuthor(reader, "executive.data.distribute");
        const fresh = await reportAccess(a, id);
        if (fresh.share?.version !== x.share?.version || fresh.r.version !== x.r.version)
            throw new BusinessRuleError(409, "REPORT_CHANGED", "Report access or design changed. Refresh to continue.");
        const documents = Object.fromEntries(x.definition.widgets.filter(w => w.kind === "html").map(w => [w.id, ports.htmlWidgetDocument(w, results[metricKey(w)], x.definition.accent)]));
        return { metrics: results, comparisons, documents, updatedAt: ports.clock.now(), version: x.r.version, shareVersion: x.share?.version ?? 0, mode: x.share?.mode ?? "owner" };
    }
    return { reportAccess, reportView, reportLibrary, createReport, saveReport, restoreReport, recipients, saveShare, recipientPreviewContexts, previewShare, ownershipCandidates, transferReportOwnership, revokeShare, reportData };
}
