import {withExpenseWorkspaceRecordRead} from '@erpx/server-people_payroll/public/record-purchases';
import {workSourceReadScope,requireWorkWorkspaceRecordRead} from "@erpx/server-operations/public/record-purchases";
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
import "server-only";
import { z } from "zod";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
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
function application(provided?: SqlDatabase,actor?:WorkContext) {
    let database = provided!;
    let control: SqlDatabase;
    const repository: RecordsRepository<SqlStatement> = { async readListRecordsResult(moduleId: string, sectionId: string, args: string[], selection: RecordScopeSelection) {
            return withExpenseWorkspaceRecordRead(database,actor,async database=>{
            if(moduleId==="finance"&&actor)await requireFinanceWorkspaceHistory(database,actor);
            const scope = scopeSql(selection),basePeople=actor?await peopleSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},commerce=actor?await commerceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},payroll=actor?await payrollSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},count=actor?await countSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},procurement=actor?await procurementSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},transfer=actor?await transferSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},projects=actor?await projectSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},quality=actor?await qualitySourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},learning=actor?await learningSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},assets=actor?await assetSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},landedCost=actor?await landedCostSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},manufacturing=actor?await manufacturingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},talent=actor?await talentSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},legal=actor?await legalSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},crm=actor?await crmSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},marketing=actor?await marketingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},followups=actor?await followupSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},workforce=actor?await workforceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},benefits=actor?await benefitSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},service=actor?await serviceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},work=actor?await workSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]};
            return database.prepare(`SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at
    FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${service.where} AND ${work.where} ORDER BY updated_at DESC,id`)
                .bind(moduleId, sectionId, ...args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...service.args,...work.args).all<RecordRow>();

            });
        },
        async readGetRecordRow(moduleId: string, id: string, args: string[], selection: RecordScopeSelection) {
            if(moduleId==="finance"&&actor)await requireFinanceWorkspaceHistory(database,actor);
            if(actor){await requireCommerceWorkspaceRecordRead(database,actor,moduleId,id);await requirePayrollWorkspaceRecordRead(database,actor,moduleId,id);await requirePeopleWorkspaceRecordRead(database,actor,moduleId,id);await requireCountWorkspaceRecordRead(database,actor,moduleId,id);await requireProcurementWorkspaceRecordRead(database,actor,moduleId,id);await requireTransferWorkspaceRecordRead(database,actor,moduleId,id);await requireProjectWorkspaceRecordRead(database,actor,moduleId,id);await requireQualityWorkspaceRecordRead(database,actor,moduleId,id);await requireLearningWorkspaceRecordRead(database,actor,moduleId,id);await requireAssetWorkspaceRecordRead(database,actor,moduleId,id);await requireLandedCostWorkspaceRecordRead(database,actor,moduleId,id);await requireManufacturingWorkspaceRecordRead(database,actor,moduleId,id);await requireTalentWorkspaceRecordRead(database,actor,moduleId,id);await requireLegalWorkspaceRecordRead(database,actor,moduleId,id);await requireCRMWorkspaceRecordRead(database,actor,moduleId,id);await requireMarketingWorkspaceRecordRead(database,actor,moduleId,id);await requireFollowupWorkspaceRecordRead(database,actor,moduleId,id);await requireWorkforceWorkspaceRecordRead(database,actor,moduleId,id);await requireBenefitWorkspaceRecordRead(database,actor,moduleId,id);await requireServiceWorkspaceRecordRead(database,actor,moduleId,id);await requireWorkWorkspaceRecordRead(database,actor,moduleId,id);}
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
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, prepareControl: async () => { control = (await runtimeEnv()).CONTROL_DB; }, hash: async (value) => { const bytes = new TextEncoder().encode(JSON.stringify(value)); return [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(item => item.toString(16).padStart(2, '0')).join(''); }, pageRecords: async (actor, moduleId, sectionId, query) => {
            return withExpenseWorkspaceRecordRead(database,actor,async database=>{
            if(moduleId==="finance")await requireFinanceWorkspaceHistory(database,actor);
            const scope = scopeSql(actor),basePeople=await peopleSourceReadScope(database,actor,"records"),commerce=await commerceSourceReadScope(database,actor,"records"),payroll=await payrollSourceReadScope(database,actor,"records"),count=await countSourceReadScope(database,actor,"records"),procurement=await procurementSourceReadScope(database,actor,"records"),transfer=await transferSourceReadScope(database,actor,"records"),projects=await projectSourceReadScope(database,actor,"records"),quality=await qualitySourceReadScope(database,actor,"records"),learning=await learningSourceReadScope(database,actor,"records"),assets=await assetSourceReadScope(database,actor,"records"),landedCost=await landedCostSourceReadScope(database,actor,"records"),manufacturing=await manufacturingSourceReadScope(database,actor,"records"),talent=await talentSourceReadScope(database,actor,"records"),legal=await legalSourceReadScope(database,actor,"records"),crm=await crmSourceReadScope(database,actor,"records"),marketing=await marketingSourceReadScope(database,actor,"records"),followups=await followupSourceReadScope(database,actor,"records"),workforce=await workforceSourceReadScope(database,actor,"records"),benefits=await benefitSourceReadScope(database,actor,"records"),service=await serviceSourceReadScope(database,actor,"records"),work=await workSourceReadScope(database,actor,"records");
            return readListPage<RecordRow & {
                page_key: string;
            }>(database, { sql: `SELECT module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at,updated_at::text page_key FROM workspace_records WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${service.where} AND ${work.where} AND strpos(lower(title||' '||COALESCE(description,'')),lower(?))>0`, args: [moduleId, sectionId, ...scope.args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...service.args,...work.args, query.q ?? ''], cursor: query.cursor, descending: true, scope: [actor.companyId, actor.userId, scope, moduleId, sectionId, query.q ?? ''] });

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
export async function createRecord(actor: WorkContext, moduleId: string, input: z.infer<typeof createRecordSchema>, key = crypto.randomUUID()) { return invoke(() => application(undefined,actor).createRecord(actor, moduleId, input, key)); }
export async function archiveRecord(actor: WorkContext, moduleId: string, id: string, expectedVersion: number, key = crypto.randomUUID()) { return invoke(() => application(undefined,actor).archiveRecord(actor, moduleId, id, expectedVersion, key)); }
export async function updateRecord(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof updateRecordSchema>, key = crypto.randomUUID()) { return invoke(() => application(undefined,actor).updateRecord(actor, moduleId, id, input, key)); }
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
