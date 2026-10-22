import { findModule, moduleAvailability } from "@erpx/catalogue/module-registry";
import type { RecordScopeSelection } from "@erpx/server-records/domain/records";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { z } from "zod";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { readListPage } from "@erpx/server-work_projection/public/list-page";
import { type RecordField, type WorkspaceRecord, type RecordRow, updateRecordSchema, createRecordSchema, transitionSchema, addReferenceSchema, suggestionSchema, suggestionDecisionSchema, addPrincipalMentionSchema, nextStatus } from '@erpx/server-records/domain/records';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { RecordsCommands } from '@erpx/server-records/domain/commands';
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
export interface RecordsRepository<Effect> {
    readListRecordsResult(moduleId: string, sectionId: string, args: string[], scope: RecordScopeSelection): Promise<{
        results: (RecordRow)[];
    }>;
    readGetRecordRow(moduleId: string, id: string, args: string[], scope: RecordScopeSelection): Promise<RecordRow | null>;
    guard3(key: string, value: "record-version", moduleId: string, id: string, version: number): Effect;
    stageCreateRecordEffect(moduleId: string, id: string, sectionId: string, title: string, description: string, value: string, companyId: string, branchId: string | null, departmentId: string | null, userId: string, userId2: string, now: number, now2: number): Effect;
    stageArchiveRecordEffect(now: number, userId: string, now2: number, moduleId: string, id: string): Effect;
    stageUpdateRecordEffect(value: string, userId: string, now: number, moduleId: string, id: string): Effect;
    stageTransitionRecordUpdate(status: "draft" | "in_review" | "approved" | "completed" | "rejected", userId: string, now: number, moduleId: string, id: string, version: number): Effect;
    stageTransitionRecordHistory(transitionId: string, moduleId: string, id: string, status: "draft" | "in_review" | "approved" | "completed" | "rejected", status2: "draft" | "in_review" | "approved" | "completed" | "rejected", action: "submit" | "approve" | "reject" | "complete" | "reopen", note: string, userId: string, assignmentId: string, value: number, now: number): Effect;
    guard9(key: string, value: "record-version", companyId: string, moduleId: string, id: string, version: number): Effect;
    readRecordContextEdgesbacklinksmentionssuggestionshistory(moduleId: string, id: string): Promise<{
        results: ({
            target_module: string;
            target_id: string;
            kind: string;
            ordinal: number;
        })[];
    }>;
    readRecordContextEdgesbacklinksmentionssuggestionshistory2(moduleId: string, id: string): Promise<{
        results: ({
            host_module: string;
            host_id: string;
            kind: string;
        })[];
    }>;
    readRecordContextEdgesbacklinksmentionssuggestionshistory3(moduleId: string, id: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readRecordContextEdgesbacklinksmentionssuggestionshistory4(moduleId: string, id: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readRecordContextEdgesbacklinksmentionssuggestionshistory5(moduleId: string, id: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readSearchReferencesResult(args: string[], modules: string[], term: string, term2: string, scope: RecordScopeSelection, placeholders: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readSearchReferencesPeople(value: string): Promise<{
        results: ({
            user_id: string;
            display_name: string;
        })[];
    }>;
    readSearchReferencesAssignment(user_id: string, now: number, now2: number): Promise<Record<string, unknown> | null>;
    readAddPrincipalMentionEligible(targetId: string, now: number, now2: number): Promise<Record<string, unknown> | null>;
    readAddPrincipalMentionPrincipal(targetId: string): Promise<{
        display_name: string;
    } | null>;
    stageAddPrincipalMentionEffect(moduleId: string, id: string, targetKind: "person", targetId: string, display_name: string, userId: string, now: number): Effect;
    readAddReferenceCount(moduleId: string, id: string): Promise<{
        count: number;
    } | null>;
    stageAddReferenceEffect(moduleId: string, id: string, targetModule: string, targetId: string, kind: "dependsOn" | "blocks" | "billedBy" | "fulfills" | "authoredWith", moduleId2: string, id2: string, userId: string, now: number): Effect;
    stageRequestSuggestionEffect(suggestionId: string, moduleId: string, id: string, key: string, value: string, value2: string, value3: string, value4: string, value5: "ready", userId: string, now: number, now2: number): Effect;
    guard24(key: string, value: "suggestion-ready", suggestionId: string): Effect;
    guard25(key: string, value: string, mod: string, sourceId: string, version: number): Effect;
    stageDecideSuggestionEffect(value: string, userId: string, now: number, moduleId: string, id: string): Effect;
    stageDecideSuggestionEffect2(value: "rejected" | "applied", userId: string, now: number, suggestionId: string): Effect;
}
export interface RecordsPorts<Effect> {
    repository: RecordsRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: RecordsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    prepareControl(): Promise<void>;
    hash(value: unknown): Promise<string>;
    pageRecords(actor: WorkContext, moduleId: string, sectionId: string, query: {
        q?: string;
        cursor?: string;
    }): ReturnType<typeof readListPage<RecordRow & {
        page_key: string;
    }>>;
    readListPage(...args: Tail<Parameters<typeof readListPage>>): PortResult<ReturnType<typeof readListPage>, Effect>;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
}
export function createRecordsApplication<Effect>(ports: RecordsPorts<Effect>) {
    const r = ports.repository;
    function assertModule(actor: WorkContext, moduleId: string) {
        if (!findModule(moduleId))
            throw new BusinessRuleError(404, "NOT_FOUND", "Module not found.");
        ports.requirePermission(actor, "records.read");
        if (!hasModuleAccess(actor.modules, moduleId))
            throw new BusinessRuleError(403, "MODULE_FORBIDDEN", "Change to a role with access to this module.");
    }
    function assertFinanceOwnerOperation(moduleId: string, actor?: WorkContext) {
        if (moduleId === "legal")
            throw new BusinessRuleError(409, "LEGAL_OWNER_REQUIRED", "Use Legal workflows; preview records remain read-only.");
        const definition = findModule(moduleId);
        if (definition && moduleAvailability(definition) === "preview")
            throw new BusinessRuleError(409, "MODULE_PREVIEW_ONLY", "This module is a preview and cannot execute business commands.");
        if (actor)
            ports.requirePermission(actor, "records.write");
        if (moduleId === "finance")
            throw new BusinessRuleError(409, "FINANCE_OWNER_REQUIRED", "Use the Finance API for accounting records; the generic workbench is read-only.");
    }
    function selectedScope(actor: WorkContext) { return { companyId: actor.companyId, branchId: actor.branchId, departmentId: actor.departmentId, args: actor.departmentId ? [actor.companyId, actor.departmentId] : actor.branchId ? [actor.companyId, actor.branchId] : [actor.companyId] }; }
    function mapRecord(row: RecordRow): WorkspaceRecord {
        return { moduleId: row.module_id, id: row.id, sectionId: row.section_id, title: row.title,
            description: row.description, status: row.status, fields: JSON.parse(row.fields_json) as RecordField[],
            version: row.version, createdBy: row.created_by, updatedAt: row.updated_at };
    }
    async function listRecords(actor: WorkContext, moduleId: string, sectionId: string) {
        assertModule(actor, moduleId);
        await ports.prepare(actor.holdingId);
        const scope = selectedScope(actor);
        const result = await r.readListRecordsResult(moduleId, sectionId, scope.args, scope);
        return result.results.map(mapRecord);
    }
    async function getRecord(actor: WorkContext, moduleId: string, id: string) {
        assertModule(actor, moduleId);
        await ports.prepare(actor.holdingId);
        const scope = selectedScope(actor);
        const row = await r.readGetRecordRow(moduleId, id, scope.args, scope);
        if (!row)
            throw new BusinessRuleError(404, "NOT_FOUND", "Record not found in this workspace.");
        return mapRecord(row);
    }
    function recordGuard(key: string, moduleId: string, id: string, version: number) {
        return r.guard3(key, "record-version", moduleId, id, version);
    }
    async function recordMutation(actor: WorkContext, moduleId: string, id: string, version: number, operation: string, input: unknown, statements: Effect[], key = ports.identifiers.next()) {
        return ports.commands.commit(actor, key, operation, { moduleId, ...(input as object) }, id, version, { id }, [recordGuard(key, moduleId, id, version), ...statements]);
    }
    async function createRecord(actor: WorkContext, moduleId: string, input: z.infer<typeof createRecordSchema>, key = ports.identifiers.next()) {
        assertFinanceOwnerOperation(moduleId, actor);
        assertModule(actor, moduleId);
        const definition = findModule(moduleId)!;
        if (!definition.sections.some(section => section.id === input.sectionId))
            throw new BusinessRuleError(422, "INVALID_SECTION", "Choose a valid module section.");
        if (new Set(input.fields.map(field => field.key)).size !== input.fields.length)
            throw new BusinessRuleError(422, "DUPLICATE_FIELD", "Field keys must be unique.");
        await ports.prepare(actor.holdingId);
        const now = ports.clock.now();
        const id = `rec-${moduleId}-${key}`;
        await ports.commands.commit(actor, key, "record.create", { moduleId, ...input }, id, 1, { id }, [
            r.stageCreateRecordEffect(moduleId, id, input.sectionId, input.title, input.description, JSON.stringify(input.fields), actor.companyId, actor.branchId, actor.departmentId, actor.userId, actor.userId, now, now)
        ]);
        return getRecord(actor, moduleId, id);
    }
    async function archiveRecord(actor: WorkContext, moduleId: string, id: string, expectedVersion: number, key = ports.identifiers.next()) {
        assertFinanceOwnerOperation(moduleId, actor);
        assertModule(actor, moduleId);
        await ports.prepare(actor.holdingId);
        if (await ports.commands.replay(actor, key, "record.archive", id, { moduleId, expectedVersion }))
            return { id, moduleId, archived: true };
        await getRecord(actor, moduleId, id);
        await recordMutation(actor, moduleId, id, expectedVersion, "record.archive", { expectedVersion }, [r.stageArchiveRecordEffect(ports.clock.now(), actor.userId, ports.clock.now(), moduleId, id)], key);
        return { id, moduleId, archived: true };
    }
    async function updateRecord(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof updateRecordSchema>, key = ports.identifiers.next()) {
        assertFinanceOwnerOperation(moduleId, actor);
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, "record.update", id, { moduleId, ...input });
        if (previous)
            return getRecord(actor, moduleId, id);
        const current = await getRecord(actor, moduleId, id);
        if (current.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "VERSION_CONFLICT", "This record changed. Refresh and review it again.");
        const allowed = new Map(current.fields.map(field => [field.key, field]));
        for (const field of Object.keys(input.fields))
            if (!allowed.has(field))
                throw new BusinessRuleError(422, "UNKNOWN_FIELD", `Unknown field: ${field}`);
        const fields = current.fields.map(field => keyValue(field, input.fields));
        await recordMutation(actor, moduleId, id, current.version, "record.update", input, [r.stageUpdateRecordEffect(JSON.stringify(fields), actor.userId, ports.clock.now(), moduleId, id)], key);
        return getRecord(actor, moduleId, id);
    }
    function keyValue(field: RecordField, values: Record<string, string | number | boolean>) {
        return Object.prototype.hasOwnProperty.call(values, field.key) ? { ...field, value: values[field.key] } : field;
    }
    async function transitionRecord(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof transitionSchema>, key: string = ports.identifiers.next()) {
        assertFinanceOwnerOperation(moduleId, actor);
        if (["approve", "reject"].includes(input.action))
            actor = ports.requirePermission(actor, "records.review");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, `record.${input.action}`, id, input);
        if (previous)
            return getRecord(actor, moduleId, id);
        const current = await getRecord(actor, moduleId, id);
        if (current.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "VERSION_CONFLICT", "This record changed. Refresh and review it again.");
        const status = nextStatus[current.status]?.[input.action];
        if (!status)
            throw new BusinessRuleError(409, "INVALID_TRANSITION", `${input.action} is unavailable while this record is ${current.status}.`);
        if (["approve", "reject"].includes(input.action)) {
            if (!(hasPermission(actor, "records.review")))
                throw new BusinessRuleError(403, "REVIEW_FORBIDDEN", "An owner or manager assignment is required to review this record.");
            if (samePerson(actor, current.createdBy))
                throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must review this record.");
        }
        const now = ports.clock.now();
        const transitionId = key;
        const update = r.stageTransitionRecordUpdate(status, actor.userId, now, moduleId, id, current.version);
        const history = r.stageTransitionRecordHistory(transitionId, moduleId, id, current.status, status, input.action, input.note, actor.userId, actor.assignmentId, current.version + 1, now);
        await ports.commands.commit(actor, key, `record.${input.action}`, input, id, current.version + 1, { id, status, version: current.version + 1 }, [
            r.guard9(key, "record-version", actor.companyId, moduleId, id, current.version), update, history,
        ]);
        return getRecord(actor, moduleId, id);
    }
    async function recordContext(actor: WorkContext, moduleId: string, id: string) {
        const record = await getRecord(actor, moduleId, id);
        await ports.prepare(actor.holdingId);
        const [edges, backlinks, mentions, suggestions, history] = await Promise.all([
            r.readRecordContextEdgesbacklinksmentionssuggestionshistory(moduleId, id),
            r.readRecordContextEdgesbacklinksmentionssuggestionshistory2(moduleId, id),
            r.readRecordContextEdgesbacklinksmentionssuggestionshistory3(moduleId, id),
            r.readRecordContextEdgesbacklinksmentionssuggestionshistory4(moduleId, id),
            r.readRecordContextEdgesbacklinksmentionssuggestionshistory5(moduleId, id),
        ]);
        const references = await Promise.all(edges.results.map(async (edge) => {
            try {
                const target = await getRecord(actor, edge.target_module, edge.target_id);
                return { ...edge, title: target.title, version: target.version, access: "available" };
            }
            catch {
                return { ...edge, title: "Unavailable record", version: 0, access: "denied" };
            }
        }));
        const visibleBacklinks = (await Promise.all(backlinks.results.map(async (edge) => {
            try {
                const host = await getRecord(actor, edge.host_module, edge.host_id);
                return { ...edge, title: host.title, version: host.version };
            }
            catch {
                return null;
            }
        }))).filter(Boolean);
        return { record, references, backlinks: visibleBacklinks, mentions: mentions.results, suggestions: suggestions.results, history: history.results };
    }
    async function searchReferences(actor: WorkContext, query: string) {
        await ports.prepare(actor.holdingId);
        const scope = selectedScope(actor);
        const modules = actor.modules.filter((id) => findModule(id));
        if (!modules.length)
            return [];
        const placeholders = modules.map(() => "?").join(",");
        const term = `%${query.replace(/^@/, "")}%`;
        const result = await r.readSearchReferencesResult(scope.args, modules, term, term, scope, placeholders);
        await ports.prepareControl();
        const needle = query.replace(/^@(?:user\/)?/, "").toLowerCase();
        const people = await r.readSearchReferencesPeople(`%${needle}%`);
        const eligiblePeople = [];
        for (const person of people.results) {
            const assignment = await r.readSearchReferencesAssignment(person.user_id, ports.clock.now(), ports.clock.now());
            if (assignment)
                eligiblePeople.push({ kind: "person", target_id: person.user_id, label: person.display_name });
        }
        return [...result.results.map(item => ({ kind: "record", ...item })), ...eligiblePeople].slice(0, 12);
    }
    async function addPrincipalMention(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof addPrincipalMentionSchema>, key = ports.identifiers.next()) {
        const current = await getRecord(actor, moduleId, id);
        await ports.prepare(actor.holdingId);
        const now = ports.clock.now();
        const eligible = await r.readAddPrincipalMentionEligible(input.targetId, now, now);
        if (!eligible)
            throw new BusinessRuleError(404, "MENTION_TARGET_NOT_FOUND", "That person is not available in this Holding.");
        await ports.prepareControl();
        const principal = await r.readAddPrincipalMentionPrincipal(input.targetId);
        if (!principal)
            throw new BusinessRuleError(404, "MENTION_TARGET_NOT_FOUND", "That person is unavailable.");
        await recordMutation(actor, moduleId, id, current.version, "record.mention", input, [r.stageAddPrincipalMentionEffect(moduleId, id, input.targetKind, input.targetId, principal.display_name, actor.userId, now)], key);
        return recordContext(actor, moduleId, id);
    }
    async function addReference(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof addReferenceSchema>, key = ports.identifiers.next()) {
        const current = await getRecord(actor, moduleId, id);
        await getRecord(actor, input.targetModule, input.targetId);
        if (moduleId === input.targetModule && id === input.targetId)
            throw new BusinessRuleError(422, "SELF_REFERENCE", "A record cannot reference itself.");
        await ports.prepare(actor.holdingId);
        const count = await r.readAddReferenceCount(moduleId, id);
        if ((count?.count ?? 0) >= 50)
            throw new BusinessRuleError(409, "REFERENCE_LIMIT", "A record can contain up to 50 references.");
        await recordMutation(actor, moduleId, id, current.version, "record.reference", input, [r.stageAddReferenceEffect(moduleId, id, input.targetModule, input.targetId, input.kind, moduleId, id, actor.userId, ports.clock.now())], key);
        return recordContext(actor, moduleId, id);
    }
    async function digest(value: unknown) { return ports.hash(value); }
    async function requestSuggestion(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof suggestionSchema>, key = ports.identifiers.next()) {
        const context = await recordContext(actor, moduleId, id);
        const field = context.record.fields.find((item) => item.key === input.fieldKey);
        if (!field)
            throw new BusinessRuleError(422, "UNKNOWN_FIELD", "Choose an editable field.");
        const refs = (context.references as Array<{
            target_module: string;
            target_id: string;
            title: string;
            version: number;
            access: string;
        }>).filter(ref => ref.access === "available");
        if (!refs.length)
            throw new BusinessRuleError(409, "EVIDENCE_REQUIRED", "Add at least one readable @ reference before requesting a suggestion.");
        const proposed = field.type === "number" ? refs.length : `${refs.map((ref) => `@${ref.target_module}/${ref.target_id}`).join(", ")} — reviewed for ${field.label.toLowerCase()}`;
        const now = ports.clock.now(), suggestionId = ports.identifiers.next();
        await ports.prepare(actor.holdingId);
        const sources = refs.map((ref) => ({ record: `${ref.target_module}/${ref.target_id}`, version: ref.version }));
        const provenance = { provider_kind: "development", model_ref: "erpx-evidence-assistant-v1", prompt_digest: await digest({ moduleId, field: field.key, sources }) };
        await recordMutation(actor, moduleId, id, context.record.version, "record.suggestion", input, [r.stageRequestSuggestionEffect(suggestionId, moduleId, id, field.key, JSON.stringify(proposed), await digest(field.value), JSON.stringify(sources), JSON.stringify(provenance), "ready", actor.userId, now, now)], key);
        return recordContext(actor, moduleId, id);
    }
    async function decideSuggestion(actor: WorkContext, moduleId: string, id: string, suggestionId: string, input: z.infer<typeof suggestionDecisionSchema>, key = ports.identifiers.next()) {
        await ports.prepare(actor.holdingId);
        if (await ports.commands.replay(actor, key, "record.suggestion.decide", id, { moduleId, suggestionId, ...input }))
            return recordContext(actor, moduleId, id);
        const context = await recordContext(actor, moduleId, id), suggestion = (context.suggestions as Array<Record<string, unknown>>).find(item => item.id === suggestionId);
        if (!suggestion || suggestion.state !== "ready")
            throw new BusinessRuleError(409, "SUGGESTION_UNAVAILABLE", "This suggestion is no longer ready.");
        const now = ports.clock.now();
        if (context.record.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "SUGGESTION_CONFLICT", "The record changed. Request a new suggestion.");
        const statements: Effect[] = [r.guard24(key, "suggestion-ready", suggestionId)];
        if (input.action === "apply") {
            assertFinanceOwnerOperation(moduleId, actor);
            for (const source of JSON.parse(String(suggestion.source_versions_json)) as Array<{
                record: string;
                version: number;
            }>) {
                const [mod, ...rest] = source.record.split("/"), sourceId = rest.join("/");
                await getRecord(actor, mod, sourceId);
                statements.push(r.guard25(key, `source-${statements.length}`, mod, sourceId, source.version));
            }
            const value = JSON.parse(String(suggestion.proposed_value_json)) as string | number | boolean;
            const fields = context.record.fields.map(field => keyValue(field, { [String(suggestion.field_key)]: value }));
            statements.push(r.stageDecideSuggestionEffect(JSON.stringify(fields), actor.userId, now, moduleId, id));
        }
        statements.push(r.stageDecideSuggestionEffect2(input.action === "apply" ? "applied" : "rejected", actor.userId, now, suggestionId));
        await recordMutation(actor, moduleId, id, context.record.version, "record.suggestion.decide", { suggestionId, ...input }, statements, key);
        return recordContext(actor, moduleId, id);
    }
    async function recordsPage(actor: WorkContext, moduleId: string, sectionId: string, query: {
        q?: string;
        cursor?: string;
    }) {
        assertModule(actor, moduleId);
        await ports.prepare(actor.holdingId);
        const page = await ports.pageRecords(actor, moduleId, sectionId, query);
        return { ...page, items: page.items.map(mapRecord) };
    }
    return { listRecords, getRecord, createRecord, archiveRecord, updateRecord, transitionRecord, recordContext, searchReferences, addPrincipalMention, addReference, requestSuggestion, decideSuggestion, recordsPage };
}
