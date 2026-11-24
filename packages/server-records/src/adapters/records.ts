import {withExpenseWorkspaceRecordRead} from '@erpx/server-people_payroll/public/record-purchases';
import {workSourceReadScope,requireWorkWorkspaceRecordRead} from "@erpx/server-operations/public/record-purchases";
// Removal task for the retained Field partition below: retire.records.registry-fallback.
// The generic engine still routes field_service reads; migration 1095 preserved legacy
// Field preview rows in workspace_records. Do not drop these guards before that task.
import {serviceSourceReadScope,requireServiceWorkspaceRecordRead} from "@erpx/server-field_service/public/record-purchases";
import {benefitSourceReadScope,requireBenefitWorkspaceRecordRead} from "@erpx/server-benefits/public/record-purchases";
import {workforceSourceReadScope,requireWorkforceWorkspaceRecordRead} from "@erpx/server-people_payroll/public/workforce-record-purchases";
import {followupSourceReadScope,requireFollowupWorkspaceRecordRead} from "@erpx/server-sales/public/followup-purchases";
import {crmSourceReadScope,requireCRMWorkspaceRecordRead} from "@erpx/server-crm/public/record-purchases";
import {marketingSourceReadScope,requireMarketingWorkspaceRecordRead} from "@erpx/server-marketing/public/record-purchases";
import {legalSourceReadScope,requireLegalWorkspaceRecordRead} from "@erpx/server-legal/public/record-purchases";
import {talentSourceReadScope,requireTalentWorkspaceRecordRead} from "@erpx/server-talent/public/record-purchases";
import {manufacturingSourceReadScope,requireManufacturingWorkspaceRecordRead} from "@erpx/server-manufacturing/public/record-purchases";
import {landedCostSourceReadScope,requireLandedCostWorkspaceRecordRead} from "@erpx/server-inventory/public/landed-cost-record-purchases";
import {assetSourceReadScope,requireAssetWorkspaceRecordRead} from "@erpx/server-assets/public/record-purchases";
import {learningSourceReadScope,requireLearningWorkspaceRecordRead} from "@erpx/server-learning/public/record-purchases";
import {qualitySourceReadScope,requireQualityWorkspaceRecordRead} from "@erpx/server-quality/public/record-purchases";
import {projectSourceReadScope,requireProjectWorkspaceRecordRead} from "@erpx/server-projects/public/record-purchases";
import {transferSourceReadScope,requireTransferWorkspaceRecordRead} from "@erpx/server-inventory/public/transfer-record-purchases";
import {procurementSourceReadScope,requireProcurementWorkspaceRecordRead} from "@erpx/server-procurement/public/record-purchases";
import {countSourceReadScope,requireCountWorkspaceRecordRead} from "@erpx/server-inventory/public/count-record-purchases";
import {peopleSourceReadScope,requirePeopleWorkspaceRecordRead,payrollSourceReadScope,requirePayrollWorkspaceRecordRead} from "@erpx/server-people_payroll/public/record-purchases";
import {commerceSourceReadScope,requireCommerceWorkspaceRecordRead} from "@erpx/server-commerce/public/record-purchases";
import {requireFinanceWorkspaceHistory,financeWorkspaceSearchScope} from "@erpx/server-finance/public/document-purchases";
import { RecordsIdentityAccessOwnedFacts } from '@erpx/server-identity_access/public/records-records-owned-facts';
function scopeSql(actor: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>) {
    if (actor.departmentId)
        return { sql: "company_id=? AND department_id=?", args: [actor.companyId, actor.departmentId] };
    if (actor.branchId)
        return { sql: "company_id=? AND branch_id=?", args: [actor.companyId, actor.branchId] };
    return { sql: "company_id=?", args: [actor.companyId] };
}
import type { RecordScopeSelection } from "@erpx/server-records/domain/records";
import { readListPage } from "@erpx/server-work_projection/public/list-page";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { existsGuardSql } from './read-tail-exists-sql';
import "server-only";
import { z } from "zod";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { nativeRecordsStatement } from '@erpx/feature-records/adapters/server/native-records-command';
import { type RecordRow, updateRecordSchema, createRecordSchema, transitionSchema, addReferenceSchema, suggestionSchema, suggestionDecisionSchema, addPrincipalMentionSchema } from '@erpx/server-records/domain/records';
import { createRecordsApplication, type RecordsRepository } from '@erpx/server-records/application/records';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { RecordsIdentityAccessFacts } from '@erpx/server-identity_access/public/records-records-facts';
export type { RecordField } from '@erpx/server-records/domain/records';
export type { WorkspaceRecord } from '@erpx/server-records/domain/records';
export { updateRecordSchema } from '@erpx/server-records/domain/records';
export { createRecordSchema } from '@erpx/server-records/domain/records';
export { transitionSchema } from '@erpx/server-records/domain/records';
export { addReferenceSchema } from '@erpx/server-records/domain/records';
export { suggestionSchema } from '@erpx/server-records/domain/records';
export { suggestionDecisionSchema } from '@erpx/server-records/domain/records';
export { addPrincipalMentionSchema } from '@erpx/server-records/domain/records';
// Shared owner-local version fence for record guards (guard3/guard25); identical SQL.
const recordVersionExistsSql = existsGuardSql('workspace_records', 'module_id=? AND id=? AND version=? AND archived_at IS NULL');
// Shared owner-local workspace-record projection for list/get/page reads; identical SQL.
const recordRowSelectSql = "SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at";
// Retained record_context edges/count consumers share one parameterized family.
// Each call emits byte-identical prior SQL; only the admitted literal is shared.
const referenceEdgesSql = (scope: 'host' | 'target', shape: 'references' | 'backlinks' | 'count') => {
    const projection = shape === 'references' ? 'target_module,target_id,kind,ordinal'
        : shape === 'backlinks' ? 'host_module,host_id,kind' : 'COUNT(*) AS count';
    const where = scope === 'host' ? 'host_module=? AND host_id=?' : 'target_module=? AND target_id=?';
    const order = shape === 'references' ? ' ORDER BY ordinal'
        : shape === 'backlinks' ? ' ORDER BY created_at DESC LIMIT 50' : '';
    return `SELECT ${projection} FROM record_reference_edges WHERE ${where}${order}`;
};
// Retained record_context detail consumers (mentions/suggestions/transitions)
// share one parameterized family; each call renders byte-identical prior SQL.
const recordContextDetailSql = (table: string, columns: string, where: string, order: string) =>
    `SELECT ${columns} FROM ${table} WHERE ${where}${order}`;
function application(provided?: SqlDatabase,actor?:WorkContext) {
    let database = provided!;
    let control: SqlDatabase;
    const repository: RecordsRepository<SqlStatement> = { async readListRecordsResult(moduleId: string, sectionId: string, args: string[], selection: RecordScopeSelection) {
            return withExpenseWorkspaceRecordRead(database,actor,async database=>{
            if(moduleId==="finance"&&actor)await requireFinanceWorkspaceHistory(database,actor);
            const scope = scopeSql(selection),basePeople=actor?await peopleSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},commerce=actor?await commerceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},payroll=actor?await payrollSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},count=actor?await countSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},procurement=actor?await procurementSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},transfer=actor?await transferSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},projects=actor?await projectSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},quality=actor?await qualitySourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},learning=actor?await learningSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},assets=actor?await assetSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},landedCost=actor?await landedCostSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},manufacturing=actor?await manufacturingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},talent=actor?await talentSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},legal=actor?await legalSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},crm=actor?await crmSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},marketing=actor?await marketingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},followups=actor?await followupSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},workforce=actor?await workforceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},benefits=actor?await benefitSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},service=actor?await serviceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},work=actor?await workSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]};
            return database.prepare(`${recordRowSelectSql}
    FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${service.where} AND ${work.where} ORDER BY updated_at DESC,id`)
                .bind(moduleId, sectionId, ...args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...service.args,...work.args).all<RecordRow>();

            });
        },
        async readGetRecordRow(moduleId: string, id: string, args: string[], selection: RecordScopeSelection) {
            if(moduleId==="finance"&&actor)await requireFinanceWorkspaceHistory(database,actor);
            if(actor){await requireCommerceWorkspaceRecordRead(database,actor,moduleId,id);await requirePayrollWorkspaceRecordRead(database,actor,moduleId,id);await requirePeopleWorkspaceRecordRead(database,actor,moduleId,id);await requireCountWorkspaceRecordRead(database,actor,moduleId,id);await requireProcurementWorkspaceRecordRead(database,actor,moduleId,id);await requireTransferWorkspaceRecordRead(database,actor,moduleId,id);await requireProjectWorkspaceRecordRead(database,actor,moduleId,id);await requireQualityWorkspaceRecordRead(database,actor,moduleId,id);await requireLearningWorkspaceRecordRead(database,actor,moduleId,id);await requireAssetWorkspaceRecordRead(database,actor,moduleId,id);await requireLandedCostWorkspaceRecordRead(database,actor,moduleId,id);await requireManufacturingWorkspaceRecordRead(database,actor,moduleId,id);await requireTalentWorkspaceRecordRead(database,actor,moduleId,id);await requireLegalWorkspaceRecordRead(database,actor,moduleId,id);await requireCRMWorkspaceRecordRead(database,actor,moduleId,id);await requireMarketingWorkspaceRecordRead(database,actor,moduleId,id);await requireFollowupWorkspaceRecordRead(database,actor,moduleId,id);await requireWorkforceWorkspaceRecordRead(database,actor,moduleId,id);await requireBenefitWorkspaceRecordRead(database,actor,moduleId,id);await requireServiceWorkspaceRecordRead(database,actor,moduleId,id);await requireWorkWorkspaceRecordRead(database,actor,moduleId,id);}
            const scope = scopeSql(selection);
            return database.prepare(`${recordRowSelectSql}
    FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND id=? AND ${scope.sql}`).bind(moduleId, id, ...args).first<RecordRow>();
        },
        guard3(key: string, value: "record-version", moduleId: string, id: string, version: number) { return assertion(database, key, value, recordVersionExistsSql, [moduleId, id, version]); },
        /** Retired: creation, archive and update execute in the native Rust records command route. */
        stageCreateRecordEffect() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records command is unavailable.'); },
        stageArchiveRecordEffect() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records command is unavailable.'); },
        stageUpdateRecordEffect() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records command is unavailable.'); },
        /** Retired: transition state and history execute in the native Rust records command route. */
        stageTransitionRecordUpdate() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records command is unavailable.'); },
        stageTransitionRecordHistory() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records command is unavailable.'); },
        guard9(key: string, value: "record-version", companyId: string, moduleId: string, id: string, version: number) { return assertion(database, key, value, existsGuardSql('workspace_records', 'company_id=? AND module_id=? AND id=? AND version=? AND archived_at IS NULL'), [companyId, moduleId, id, version]); },
        readRecordContextEdgesbacklinksmentionssuggestionshistory(moduleId: string, id: string) {
            return database.prepare(referenceEdgesSql('host','references')).bind(moduleId, id).all<{
                target_module: string;
                target_id: string;
                kind: string;
                ordinal: number;
            }>();
        },
        readRecordContextEdgesbacklinksmentionssuggestionshistory2(moduleId: string, id: string) {
            return database.prepare(referenceEdgesSql('target','backlinks')).bind(moduleId, id).all<{
                host_module: string;
                host_id: string;
                kind: string;
            }>();
        },
        readRecordContextEdgesbacklinksmentionssuggestionshistory3(moduleId: string, id: string) { return database.prepare(recordContextDetailSql('record_principal_mentions','target_kind,target_id,target_label,created_at','host_module=? AND host_id=?',' ORDER BY created_at')).bind(moduleId, id).all(); },
        readRecordContextEdgesbacklinksmentionssuggestionshistory4(moduleId: string, id: string) { return database.prepare(recordContextDetailSql('record_field_suggestions','id,field_key,proposed_value_json,source_versions_json,provenance_json,state,created_at','module_id=? AND record_id=?',' ORDER BY created_at DESC LIMIT 20')).bind(moduleId, id).all(); },
        readRecordContextEdgesbacklinksmentionssuggestionshistory5(moduleId: string, id: string) { return database.prepare(recordContextDetailSql('workspace_record_transitions','from_status,to_status,action,note,actor_id,record_version,created_at','module_id=? AND record_id=?',' ORDER BY created_at DESC LIMIT 20')).bind(moduleId, id).all(); },
        async readSearchReferencesResult(args: string[], modules: string[], term: string, term2: string, selection: RecordScopeSelection, placeholders: string) {
            return withExpenseWorkspaceRecordRead(database,actor,async database=>{
            const scope = scopeSql(selection),basePeople=actor?await peopleSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},purchase=actor?await financeWorkspaceSearchScope(database,actor):{clock:"",where:"TRUE",args:[]},commerce=actor?await commerceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},payroll=actor?await payrollSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},count=actor?await countSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},procurement=actor?await procurementSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},transfer=actor?await transferSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},projects=actor?await projectSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},quality=actor?await qualitySourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},learning=actor?await learningSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},assets=actor?await assetSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},landedCost=actor?await landedCostSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},manufacturing=actor?await manufacturingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},talent=actor?await talentSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},legal=actor?await legalSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},crm=actor?await crmSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},marketing=actor?await marketingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},followups=actor?await followupSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},workforce=actor?await workforceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},benefits=actor?await benefitSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},service=actor?await serviceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},work=actor?await workSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]};
            return database.prepare(`${purchase.clock}SELECT module_id,id,title,status,version FROM workspace_records WHERE archived_at IS NULL AND ${scope.sql}
    AND ${purchase.where} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${service.where} AND ${work.where} AND module_id IN (${placeholders}) AND (module_id||'/'||id ILIKE ? OR title ILIKE ?) ORDER BY updated_at DESC LIMIT 12`)
                .bind(...args,...purchase.args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...service.args,...work.args, ...modules, term, term2).all();

            });
        },
        readSearchReferencesPeople(...args) { return RecordsIdentityAccessOwnedFacts(control).readSearchReferencesPeople(...args); },
        readSearchReferencesAssignment(...args) { return RecordsIdentityAccessFacts(database).readSearchReferencesAssignment(...args); },
        readAddPrincipalMentionEligible(...args) { return RecordsIdentityAccessFacts(database).readAddPrincipalMentionEligible(...args); },
        readAddPrincipalMentionPrincipal(...args) { return RecordsIdentityAccessOwnedFacts(control).readAddPrincipalMentionPrincipal(...args); },
        stageAddPrincipalMentionEffect(moduleId: string, id: string, targetKind: "person", targetId: string, display_name: string, userId: string, now: number) { return nativeRecordsStatement('records_stmt_18', { companyId: actor?.companyId, actor, now: Date.now() }).bind(moduleId, id, targetKind, targetId, display_name, userId, now); },
        readAddReferenceCount(moduleId: string, id: string) {
            return database.prepare(referenceEdgesSql('host','count')).bind(moduleId, id).first<{
                count: number;
            }>();
        },
        /** Retired: record references execute in the native Rust records command route. */
        stageAddReferenceEffect() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records command is unavailable.'); },
        stageRequestSuggestionEffect(suggestionId: string, moduleId: string, id: string, key: string, value: string, value2: string, value3: string, value4: string, value5: "ready", userId: string, now: number, now2: number) {
            return nativeRecordsStatement('records_stmt_19', { companyId: actor?.companyId, actor, now: Date.now() }).bind(suggestionId, moduleId, id, key, value, value2, value3, value4, value5, userId, now, now2);
        },
        guard24(key: string, value: "suggestion-ready", suggestionId: string) { return assertion(database, key, value, existsGuardSql('record_field_suggestions', "id=? AND state='ready'"), [suggestionId]); },
        guard25(key: string, value: string, mod: string, sourceId: string, version: number) { return assertion(database, key, value, recordVersionExistsSql, [mod, sourceId, version]); },
        stageDecideSuggestionEffect(value: string, userId: string, now: number, moduleId: string, id: string) { return nativeRecordsStatement('records_stmt_20', { companyId: actor?.companyId, actor, now: Date.now() }).bind(value, userId, now, moduleId, id); },
        stageDecideSuggestionEffect2(value: "rejected" | "applied", userId: string, now: number, suggestionId: string) { return nativeRecordsStatement('records_stmt_21', { companyId: actor?.companyId, actor, now: Date.now() }).bind(value, userId, now, suggestionId); } };
    return createRecordsApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, prepareControl: async () => { control = (await runtimeEnv()).CONTROL_DB; }, hash: async (value) => { const bytes = new TextEncoder().encode(JSON.stringify(value)); return [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(item => item.toString(16).padStart(2, '0')).join(''); }, pageRecords: async (actor, moduleId, sectionId, query) => {
            return withExpenseWorkspaceRecordRead(database,actor,async database=>{
            if(moduleId==="finance")await requireFinanceWorkspaceHistory(database,actor);
            const scope = scopeSql(actor),basePeople=await peopleSourceReadScope(database,actor,"records"),commerce=await commerceSourceReadScope(database,actor,"records"),payroll=await payrollSourceReadScope(database,actor,"records"),count=await countSourceReadScope(database,actor,"records"),procurement=await procurementSourceReadScope(database,actor,"records"),transfer=await transferSourceReadScope(database,actor,"records"),projects=await projectSourceReadScope(database,actor,"records"),quality=await qualitySourceReadScope(database,actor,"records"),learning=await learningSourceReadScope(database,actor,"records"),assets=await assetSourceReadScope(database,actor,"records"),landedCost=await landedCostSourceReadScope(database,actor,"records"),manufacturing=await manufacturingSourceReadScope(database,actor,"records"),talent=await talentSourceReadScope(database,actor,"records"),legal=await legalSourceReadScope(database,actor,"records"),crm=await crmSourceReadScope(database,actor,"records"),marketing=await marketingSourceReadScope(database,actor,"records"),followups=await followupSourceReadScope(database,actor,"records"),workforce=await workforceSourceReadScope(database,actor,"records"),benefits=await benefitSourceReadScope(database,actor,"records"),service=await serviceSourceReadScope(database,actor,"records"),work=await workSourceReadScope(database,actor,"records");
            return readListPage<RecordRow & {
                page_key: string;
            }>(database, { sql: `${recordRowSelectSql},updated_at::text page_key FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${service.where} AND ${work.where} AND strpos(lower(title||' '||COALESCE(description,'')),lower(?))>0`, args: [moduleId, sectionId, ...scope.args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...service.args,...work.args, query.q ?? ''], cursor: query.cursor, descending: true, scope: [actor.companyId, actor.userId, scope, moduleId, sectionId, query.q ?? ''] });

            });
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
export async function listRecords(actor: WorkContext, moduleId: string, sectionId: string) { return invoke(() => application(undefined,actor).listRecords(actor, moduleId, sectionId)); }
export async function getRecord(actor: WorkContext, moduleId: string, id: string, provided?: SqlDatabase) { return invoke(() => application(provided,actor).getRecord(actor, moduleId, id)); }
export async function transitionRecord(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof transitionSchema>, key: string = crypto.randomUUID()) { return invoke(() => application(undefined,actor).transitionRecord(actor, moduleId, id, input, key)); }
export async function recordContext(actor: WorkContext, moduleId: string, id: string) { return invoke(() => application(undefined,actor).recordContext(actor, moduleId, id)); }
export async function searchReferences(actor: WorkContext, query: string) { return invoke(() => application(undefined,actor).searchReferences(actor, query)); }
export async function addPrincipalMention(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof addPrincipalMentionSchema>, key = crypto.randomUUID()) { return invoke(() => application(undefined,actor).addPrincipalMention(actor, moduleId, id, input, key)); }
export async function addReference(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof addReferenceSchema>, key = crypto.randomUUID()) { return invoke(() => application(undefined,actor).addReference(actor, moduleId, id, input, key)); }
export async function requestSuggestion(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof suggestionSchema>, key = crypto.randomUUID()) { return invoke(() => application(undefined,actor).requestSuggestion(actor, moduleId, id, input, key)); }
export async function decideSuggestion(actor: WorkContext, moduleId: string, id: string, suggestionId: string, input: z.infer<typeof suggestionDecisionSchema>, key = crypto.randomUUID()) { return invoke(() => application(undefined,actor).decideSuggestion(actor, moduleId, id, suggestionId, input, key)); }
export async function recordsPage(actor: WorkContext, moduleId: string, sectionId: string, query: {
    q?: string;
    cursor?: string;
}) { return invoke(() => application(undefined,actor).recordsPage(actor, moduleId, sectionId, query)); }
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
