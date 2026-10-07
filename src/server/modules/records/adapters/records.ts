import { RecordsIdentityAccessOwnedFacts } from '../../identity_access/public/records-records-owned-facts';
function scopeSql(actor: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>) {
    if (actor.departmentId)
        return { sql: "company_id=? AND department_id=?", args: [actor.companyId, actor.departmentId] };
    if (actor.branchId)
        return { sql: "company_id=? AND branch_id=?", args: [actor.companyId, actor.branchId] };
    return { sql: "company_id=?", args: [actor.companyId] };
}
import type { RecordScopeSelection } from "../domain/records";
import { readListPage } from "@/server/modules/work_projection/public/list-page";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
import "server-only";
import { z } from "zod";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { type RecordRow, updateRecordSchema, createRecordSchema, transitionSchema, addReferenceSchema, suggestionSchema, suggestionDecisionSchema, addPrincipalMentionSchema } from '../domain/records';
import { createRecordsApplication, type RecordsRepository } from '../application/records';
import { BusinessRuleError } from '../../shared';
import { RecordsIdentityAccessFacts } from '../../identity_access/public/records-records-facts';
export type { RecordField } from '../domain/records';
export type { WorkspaceRecord } from '../domain/records';
export { updateRecordSchema } from '../domain/records';
export { createRecordSchema } from '../domain/records';
export { transitionSchema } from '../domain/records';
export { addReferenceSchema } from '../domain/records';
export { suggestionSchema } from '../domain/records';
export { suggestionDecisionSchema } from '../domain/records';
export { addPrincipalMentionSchema } from '../domain/records';
function application(provided?: SqlDatabase) {
    let database = provided!;
    let control: SqlDatabase;
    const repository: RecordsRepository<SqlStatement> = { readListRecordsResult(moduleId: string, sectionId: string, args: string[], selection: RecordScopeSelection) {
            const scope = scopeSql(selection);
            return database.prepare(`SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at
    FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} ORDER BY updated_at DESC,id`)
                .bind(moduleId, sectionId, ...args).all<RecordRow>();
        },
        readGetRecordRow(moduleId: string, id: string, args: string[], selection: RecordScopeSelection) {
            const scope = scopeSql(selection);
            return database.prepare(`SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at
    FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND id=? AND ${scope.sql}`).bind(moduleId, id, ...args).first<RecordRow>();
        },
        guard3(key: string, value: "record-version", moduleId: string, id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM workspace_records WHERE module_id=? AND id=? AND version=? AND archived_at IS NULL)", [moduleId, id, version]); },
        stageCreateRecordEffect(moduleId: string, id: string, sectionId: string, title: string, description: string, value: string, companyId: string, branchId: string | null, departmentId: string | null, userId: string, userId2: string, now: number, now2: number) { return database.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,version,created_by,updated_by,created_at,updated_at) VALUES(?,?,?,?,?,'draft',?,?,?,?,1,?,?,?,?)").bind(moduleId, id, sectionId, title, description, value, companyId, branchId, departmentId, userId, userId2, now, now2); },
        stageArchiveRecordEffect(now: number, userId: string, now2: number, moduleId: string, id: string) { return database.prepare("UPDATE workspace_records SET archived_at=?,version=version+1,updated_by=?,updated_at=? WHERE module_id=? AND id=?").bind(now, userId, now2, moduleId, id); },
        stageUpdateRecordEffect(value: string, userId: string, now: number, moduleId: string, id: string) { return database.prepare("UPDATE workspace_records SET fields_json=?,version=version+1,updated_by=?,updated_at=? WHERE module_id=? AND id=?").bind(value, userId, now, moduleId, id); },
        stageTransitionRecordUpdate(status: "draft" | "in_review" | "approved" | "completed" | "rejected", userId: string, now: number, moduleId: string, id: string, version: number) {
            return database.prepare(`UPDATE workspace_records SET status=?,version=version+1,updated_by=?,updated_at=? WHERE module_id=? AND id=? AND version=?`)
                .bind(status, userId, now, moduleId, id, version);
        },
        stageTransitionRecordHistory(transitionId: string, moduleId: string, id: string, status: "draft" | "in_review" | "approved" | "completed" | "rejected", status2: "draft" | "in_review" | "approved" | "completed" | "rejected", action: "submit" | "approve" | "reject" | "complete" | "reopen", note: string, userId: string, assignmentId: string, value: number, now: number) {
            return database.prepare(`INSERT INTO workspace_record_transitions(id,module_id,record_id,from_status,to_status,action,note,actor_id,assignment_id,record_version,created_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(transitionId, moduleId, id, status, status2, action, note, userId, assignmentId, value, now);
        },
        guard9(key: string, value: "record-version", companyId: string, moduleId: string, id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM workspace_records WHERE company_id=? AND module_id=? AND id=? AND version=? AND archived_at IS NULL)", [companyId, moduleId, id, version]); },
        readRecordContextEdgesbacklinksmentionssuggestionshistory(moduleId: string, id: string) {
            return database.prepare(`SELECT target_module,target_id,kind,ordinal FROM record_reference_edges WHERE host_module=? AND host_id=? ORDER BY ordinal`).bind(moduleId, id).all<{
                target_module: string;
                target_id: string;
                kind: string;
                ordinal: number;
            }>();
        },
        readRecordContextEdgesbacklinksmentionssuggestionshistory2(moduleId: string, id: string) {
            return database.prepare(`SELECT host_module,host_id,kind FROM record_reference_edges WHERE target_module=? AND target_id=? ORDER BY created_at DESC LIMIT 50`).bind(moduleId, id).all<{
                host_module: string;
                host_id: string;
                kind: string;
            }>();
        },
        readRecordContextEdgesbacklinksmentionssuggestionshistory3(moduleId: string, id: string) { return database.prepare(`SELECT target_kind,target_id,target_label,created_at FROM record_principal_mentions WHERE host_module=? AND host_id=? ORDER BY created_at`).bind(moduleId, id).all(); },
        readRecordContextEdgesbacklinksmentionssuggestionshistory4(moduleId: string, id: string) { return database.prepare(`SELECT id,field_key,proposed_value_json,source_versions_json,provenance_json,state,created_at FROM record_field_suggestions WHERE module_id=? AND record_id=? ORDER BY created_at DESC LIMIT 20`).bind(moduleId, id).all(); },
        readRecordContextEdgesbacklinksmentionssuggestionshistory5(moduleId: string, id: string) { return database.prepare(`SELECT from_status,to_status,action,note,actor_id,record_version,created_at FROM workspace_record_transitions WHERE module_id=? AND record_id=? ORDER BY created_at DESC LIMIT 20`).bind(moduleId, id).all(); },
        readSearchReferencesResult(args: string[], modules: string[], term: string, term2: string, selection: RecordScopeSelection, placeholders: string) {
            const scope = scopeSql(selection);
            return database.prepare(`SELECT module_id,id,title,status,version FROM workspace_records WHERE archived_at IS NULL AND ${scope.sql}
    AND module_id IN (${placeholders}) AND (module_id||'/'||id ILIKE ? OR title ILIKE ?) ORDER BY updated_at DESC LIMIT 12`)
                .bind(...args, ...modules, term, term2).all();
        },
        readSearchReferencesPeople(...args) { return RecordsIdentityAccessOwnedFacts(control).readSearchReferencesPeople(...args); },
        readSearchReferencesAssignment(...args) { return RecordsIdentityAccessFacts(database).readSearchReferencesAssignment(...args); },
        readAddPrincipalMentionEligible(...args) { return RecordsIdentityAccessFacts(database).readAddPrincipalMentionEligible(...args); },
        readAddPrincipalMentionPrincipal(...args) { return RecordsIdentityAccessOwnedFacts(control).readAddPrincipalMentionPrincipal(...args); },
        stageAddPrincipalMentionEffect(moduleId: string, id: string, targetKind: "person", targetId: string, display_name: string, userId: string, now: number) { return database.prepare(`INSERT INTO record_principal_mentions(host_module,host_id,target_kind,target_id,target_label,created_by,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING`).bind(moduleId, id, targetKind, targetId, display_name, userId, now); },
        readAddReferenceCount(moduleId: string, id: string) {
            return database.prepare(`SELECT COUNT(*) AS count FROM record_reference_edges WHERE host_module=? AND host_id=?`).bind(moduleId, id).first<{
                count: number;
            }>();
        },
        stageAddReferenceEffect(moduleId: string, id: string, targetModule: string, targetId: string, kind: "dependsOn" | "blocks" | "billedBy" | "fulfills" | "authoredWith", moduleId2: string, id2: string, userId: string, now: number) {
            return database.prepare(`INSERT INTO record_reference_edges(host_module,host_id,target_module,target_id,kind,ordinal,created_by,created_at)
    VALUES(?,?,?,?,?,COALESCE((SELECT MAX(ordinal)+1 FROM record_reference_edges WHERE host_module=? AND host_id=?),0),?,?) ON CONFLICT DO NOTHING`)
                .bind(moduleId, id, targetModule, targetId, kind, moduleId2, id2, userId, now);
        },
        stageRequestSuggestionEffect(suggestionId: string, moduleId: string, id: string, key: string, value: string, value2: string, value3: string, value4: string, value5: "ready", userId: string, now: number, now2: number) {
            return database.prepare(`INSERT INTO record_field_suggestions(id,module_id,record_id,field_key,proposed_value_json,previous_value_digest,source_versions_json,provenance_json,state,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`).bind(suggestionId, moduleId, id, key, value, value2, value3, value4, value5, userId, now, now2);
        },
        guard24(key: string, value: "suggestion-ready", suggestionId: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM record_field_suggestions WHERE id=? AND state='ready')", [suggestionId]); },
        guard25(key: string, value: string, mod: string, sourceId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM workspace_records WHERE module_id=? AND id=? AND version=? AND archived_at IS NULL)", [mod, sourceId, version]); },
        stageDecideSuggestionEffect(value: string, userId: string, now: number, moduleId: string, id: string) { return database.prepare("UPDATE workspace_records SET fields_json=?,version=version+1,updated_by=?,updated_at=? WHERE module_id=? AND id=?").bind(value, userId, now, moduleId, id); },
        stageDecideSuggestionEffect2(value: "rejected" | "applied", userId: string, now: number, suggestionId: string) { return database.prepare("UPDATE record_field_suggestions SET state=?,applied_by=?,updated_at=? WHERE id=?").bind(value, userId, now, suggestionId); } };
    return createRecordsApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, prepareControl: async () => { control = (await runtimeEnv()).CONTROL_DB; }, hash: async (value) => { const bytes = new TextEncoder().encode(JSON.stringify(value)); return [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(item => item.toString(16).padStart(2, '0')).join(''); }, pageRecords: (actor, moduleId, sectionId, query) => {
            const scope = scopeSql(actor);
            return readListPage<RecordRow & {
                page_key: string;
            }>(database, { sql: `SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at,updated_at::text page_key FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} AND strpos(lower(title||' '||COALESCE(description,'')),lower(?))>0`, args: [moduleId, sectionId, ...scope.args, query.q ?? ''], cursor: query.cursor, descending: true, scope: [actor.companyId, actor.userId, scope, moduleId, sectionId, query.q ?? ''] });
        }, readListPage: (...args) => fromNative(() => readListPage(database, ...args)), requirePermission: (actor, permission) => fromNativeSync(() => requirePermission(actor, permission)) });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function listRecords(actor: WorkContext, moduleId: string, sectionId: string) { return invoke(() => application().listRecords(actor, moduleId, sectionId)); }
export async function getRecord(actor: WorkContext, moduleId: string, id: string, provided?: SqlDatabase) { return invoke(() => application(provided).getRecord(actor, moduleId, id)); }
export async function createRecord(actor: WorkContext, moduleId: string, input: z.infer<typeof createRecordSchema>, key = crypto.randomUUID()) { return invoke(() => application().createRecord(actor, moduleId, input, key)); }
export async function archiveRecord(actor: WorkContext, moduleId: string, id: string, expectedVersion: number, key = crypto.randomUUID()) { return invoke(() => application().archiveRecord(actor, moduleId, id, expectedVersion, key)); }
export async function updateRecord(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof updateRecordSchema>, key = crypto.randomUUID()) { return invoke(() => application().updateRecord(actor, moduleId, id, input, key)); }
export async function transitionRecord(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof transitionSchema>, key: string = crypto.randomUUID()) { return invoke(() => application().transitionRecord(actor, moduleId, id, input, key)); }
export async function recordContext(actor: WorkContext, moduleId: string, id: string) { return invoke(() => application().recordContext(actor, moduleId, id)); }
export async function searchReferences(actor: WorkContext, query: string) { return invoke(() => application().searchReferences(actor, query)); }
export async function addPrincipalMention(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof addPrincipalMentionSchema>, key = crypto.randomUUID()) { return invoke(() => application().addPrincipalMention(actor, moduleId, id, input, key)); }
export async function addReference(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof addReferenceSchema>, key = crypto.randomUUID()) { return invoke(() => application().addReference(actor, moduleId, id, input, key)); }
export async function requestSuggestion(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof suggestionSchema>, key = crypto.randomUUID()) { return invoke(() => application().requestSuggestion(actor, moduleId, id, input, key)); }
export async function decideSuggestion(actor: WorkContext, moduleId: string, id: string, suggestionId: string, input: z.infer<typeof suggestionDecisionSchema>, key = crypto.randomUUID()) { return invoke(() => application().decideSuggestion(actor, moduleId, id, suggestionId, input, key)); }
export async function recordsPage(actor: WorkContext, moduleId: string, sectionId: string, query: {
    q?: string;
    cursor?: string;
}) { return invoke(() => application().recordsPage(actor, moduleId, sectionId, query)); }
function fromNativeSync<Result>(run: () => Result) {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
