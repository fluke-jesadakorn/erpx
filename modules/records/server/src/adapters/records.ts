import {withExpenseWorkspaceRecordRead} from '@erpx/people-payroll-server/public/record-purchases';
import {workSourceReadScope,requireWorkWorkspaceRecordRead} from "@erpx/operations-server/public/record-purchases";
// Removal task for the retained Field partition below: retire.records.registry-fallback.
// The generic engine still routes field_service reads; migration 1095 preserved legacy
// Field preview rows in workspace_records. Do not drop these guards before that task.
import {serviceSourceReadScope,requireServiceWorkspaceRecordRead} from "@erpx/field-service-server/public/record-purchases";
import {benefitSourceReadScope,requireBenefitWorkspaceRecordRead} from "@erpx/benefits-server/public/record-purchases";
import {workforceSourceReadScope,requireWorkforceWorkspaceRecordRead} from "@erpx/people-payroll-server/public/workforce-record-purchases";
import {followupSourceReadScope,requireFollowupWorkspaceRecordRead} from "@erpx/sales-server/public/followup-purchases";
import {crmSourceReadScope,requireCRMWorkspaceRecordRead} from "@erpx/crm-server/public/record-purchases";
import {marketingSourceReadScope,requireMarketingWorkspaceRecordRead} from "@erpx/marketing-server/public/record-purchases";
import {legalSourceReadScope,requireLegalWorkspaceRecordRead} from "@erpx/legal-server/public/record-purchases";
import {talentSourceReadScope,requireTalentWorkspaceRecordRead} from "@erpx/talent-server/public/record-purchases";
import {manufacturingSourceReadScope,requireManufacturingWorkspaceRecordRead} from "@erpx/manufacturing-server/public/record-purchases";
import {landedCostSourceReadScope,requireLandedCostWorkspaceRecordRead} from "@erpx/inventory-server/public/landed-cost-record-purchases";
import {assetSourceReadScope,requireAssetWorkspaceRecordRead} from "@erpx/assets-server/public/record-purchases";
import {learningSourceReadScope,requireLearningWorkspaceRecordRead} from "@erpx/learning-server/public/record-purchases";
import {qualitySourceReadScope,requireQualityWorkspaceRecordRead} from "@erpx/quality-server/public/record-purchases";
import {projectSourceReadScope,requireProjectWorkspaceRecordRead} from "@erpx/projects-server/public/record-purchases";
import {transferSourceReadScope,requireTransferWorkspaceRecordRead} from "@erpx/inventory-server/public/transfer-record-purchases";
import {procurementSourceReadScope,requireProcurementWorkspaceRecordRead} from "@erpx/procurement-server/public/record-purchases";
import {countSourceReadScope,requireCountWorkspaceRecordRead} from "@erpx/inventory-server/public/count-record-purchases";
import {peopleSourceReadScope,requirePeopleWorkspaceRecordRead,payrollSourceReadScope,requirePayrollWorkspaceRecordRead} from "@erpx/people-payroll-server/public/record-purchases";
import {commerceSourceReadScope,requireCommerceWorkspaceRecordRead} from "@erpx/commerce-server/public/record-purchases";
import {requireFinanceWorkspaceHistory,financeWorkspaceSearchScope} from "@erpx/finance-server/public/document-purchases";
import { RecordsIdentityAccessOwnedFacts } from '@erpx/identity-access-server/public/records-records-owned-facts';
function scopeSql(actor: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>) {
    if (actor.departmentId)
        return { sql: "company_id=? AND department_id=?", args: [actor.companyId, actor.departmentId] };
    if (actor.branchId)
        return { sql: "company_id=? AND branch_id=?", args: [actor.companyId, actor.branchId] };
    return { sql: "company_id=?", args: [actor.companyId] };
}
import type { RecordScopeSelection } from "@erpx/records-server/domain/records";
import { readListPage } from "@erpx/work-projection-server/public/list-page";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import { existsGuardSql } from './read-tail-exists-sql';
import "server-only";
import { z } from "zod";
import { runtimeEnv } from "../../../../../platform/runtime/server/src/public/runtime";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources as nativeHoldingResources } from '../../../../../platform/runtime/server/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/identity-access-server/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/workflow-server/public/commands';
import { ApiError as NativeApiError } from '@erpx/platform-runtime-server/public/http';
import { createNativeRecordContextClient, nativeRecordContextConfiguration, nativeRecordContextFailure } from '../public/native-record-context';
import { projectionColumnHeadSql } from './read-tail-r32-sql';
import { nativeRecordsStatement } from '@erpx/records-ui/adapters/server/native-records-command';
import { type RecordRow, updateRecordSchema, createRecordSchema, transitionSchema, addReferenceSchema } from '@erpx/records-server/domain/records';
import { createRecordsApplication, type RecordsRepository } from '@erpx/records-server/application/records';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { RecordsIdentityAccessFacts } from '@erpx/identity-access-server/public/records-records-facts';
export type { RecordField } from '@erpx/records-server/domain/records';
export type { WorkspaceRecord } from '@erpx/records-server/domain/records';
export { updateRecordSchema } from '@erpx/records-server/domain/records';
export { createRecordSchema } from '@erpx/records-server/domain/records';
export { transitionSchema } from '@erpx/records-server/domain/records';
export { addReferenceSchema } from '@erpx/records-server/domain/records';
// The remaining generic record version guard.
const recordVersionExistsSql = existsGuardSql('workspace_records', 'module_id=? AND id=? AND version=? AND archived_at IS NULL');
// Shared owner-local workspace-record projection for list/get/page reads. The
// spaced read head now renders through the r32 renderer; the columns argument
// carries the original newline/indent so all three callers keep byte-identical SQL.
const recordRowColumns = 'module_id,id,section_id,title,description,status,fields_json,version,created_by,updated_at';
export const recordRowSelectSql = (suffix: string) =>
    projectionColumnHeadSql(`${recordRowColumns}\n   `, 'workspace_records', suffix);
export const recordRowPageSql = (suffix: string) =>
    projectionColumnHeadSql(`${recordRowColumns},updated_at::text page_key`, 'workspace_records', suffix);
function application(provided?: SqlDatabase,actor?:WorkContext) {
    let database = provided!;
    let control: SqlDatabase;
    const repository: RecordsRepository<SqlStatement> = { async readListRecordsResult(moduleId: string, sectionId: string, args: string[], selection: RecordScopeSelection) {
            return withExpenseWorkspaceRecordRead(database,actor,async database=>{
            if(moduleId==="finance"&&actor)await requireFinanceWorkspaceHistory(database,actor);
            const scope = scopeSql(selection),basePeople=actor?await peopleSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},commerce=actor?await commerceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},payroll=actor?await payrollSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},count=actor?await countSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},procurement=actor?await procurementSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},transfer=actor?await transferSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},projects=actor?await projectSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},quality=actor?await qualitySourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},learning=actor?await learningSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},assets=actor?await assetSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},landedCost=actor?await landedCostSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},manufacturing=actor?await manufacturingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},talent=actor?await talentSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},legal=actor?await legalSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},crm=actor?await crmSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},marketing=actor?await marketingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},followups=actor?await followupSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},workforce=actor?await workforceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},benefits=actor?await benefitSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},service=actor?await serviceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},work=actor?await workSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]};
            return database.prepare(recordRowSelectSql(` WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${service.where} AND ${work.where} ORDER BY updated_at DESC,id`))
                .bind(moduleId, sectionId, ...args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...service.args,...work.args).all<RecordRow>();

            });
        },
        async readGetRecordRow(moduleId: string, id: string, args: string[], selection: RecordScopeSelection) {
            if(moduleId==="finance"&&actor)await requireFinanceWorkspaceHistory(database,actor);
            if(actor){await requireCommerceWorkspaceRecordRead(database,actor,moduleId,id);await requirePayrollWorkspaceRecordRead(database,actor,moduleId,id);await requirePeopleWorkspaceRecordRead(database,actor,moduleId,id);await requireCountWorkspaceRecordRead(database,actor,moduleId,id);await requireProcurementWorkspaceRecordRead(database,actor,moduleId,id);await requireTransferWorkspaceRecordRead(database,actor,moduleId,id);await requireProjectWorkspaceRecordRead(database,actor,moduleId,id);await requireQualityWorkspaceRecordRead(database,actor,moduleId,id);await requireLearningWorkspaceRecordRead(database,actor,moduleId,id);await requireAssetWorkspaceRecordRead(database,actor,moduleId,id);await requireLandedCostWorkspaceRecordRead(database,actor,moduleId,id);await requireManufacturingWorkspaceRecordRead(database,actor,moduleId,id);await requireTalentWorkspaceRecordRead(database,actor,moduleId,id);await requireLegalWorkspaceRecordRead(database,actor,moduleId,id);await requireCRMWorkspaceRecordRead(database,actor,moduleId,id);await requireMarketingWorkspaceRecordRead(database,actor,moduleId,id);await requireFollowupWorkspaceRecordRead(database,actor,moduleId,id);await requireWorkforceWorkspaceRecordRead(database,actor,moduleId,id);await requireBenefitWorkspaceRecordRead(database,actor,moduleId,id);await requireServiceWorkspaceRecordRead(database,actor,moduleId,id);await requireWorkWorkspaceRecordRead(database,actor,moduleId,id);}
            const scope = scopeSql(selection);
            return database.prepare(recordRowSelectSql(` WHERE archived_at IS NULL AND module_id=? AND id=? AND ${scope.sql}`)).bind(moduleId, id, ...args).first<RecordRow>();
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
        /** Retired: the context read family executes in the native Rust reader. */
        readRecordContextEdgesbacklinksmentionssuggestionshistory() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records context reader is unavailable.'); },
        /** Retired: the context read family executes in the native Rust reader. */
        readRecordContextEdgesbacklinksmentionssuggestionshistory2() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records context reader is unavailable.'); },
        /** Retired: the context read family executes in the native Rust reader. */
        readRecordContextEdgesbacklinksmentionssuggestionshistory3() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records context reader is unavailable.'); },
        /** Retired: the context read family executes in the native Rust reader. */
        readRecordContextEdgesbacklinksmentionssuggestionshistory4() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records context reader is unavailable.'); },
        /** Retired: the context read family executes in the native Rust reader. */
        readRecordContextEdgesbacklinksmentionssuggestionshistory5() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records context reader is unavailable.'); },
        async readSearchReferencesResult(args: string[], modules: string[], term: string, term2: string, selection: RecordScopeSelection, placeholders: string) {
            return withExpenseWorkspaceRecordRead(database,actor,async database=>{
            const scope = scopeSql(selection),basePeople=actor?await peopleSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},purchase=actor?await financeWorkspaceSearchScope(database,actor):{clock:"",where:"TRUE",args:[]},commerce=actor?await commerceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},payroll=actor?await payrollSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},count=actor?await countSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},procurement=actor?await procurementSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},transfer=actor?await transferSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},projects=actor?await projectSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},quality=actor?await qualitySourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},learning=actor?await learningSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},assets=actor?await assetSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},landedCost=actor?await landedCostSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},manufacturing=actor?await manufacturingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},talent=actor?await talentSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},legal=actor?await legalSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},crm=actor?await crmSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},marketing=actor?await marketingSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},followups=actor?await followupSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},workforce=actor?await workforceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},benefits=actor?await benefitSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},service=actor?await serviceSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]},work=actor?await workSourceReadScope(database,actor,"records"):{where:"TRUE",args:[]};
            return database.prepare(`${purchase.clock}${projectionColumnHeadSql('module_id,id,title,status,version','workspace_records',` WHERE archived_at IS NULL AND ${scope.sql}
    AND ${purchase.where} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${service.where} AND ${work.where} AND module_id IN (${placeholders}) AND (module_id||'/'||id ILIKE ? OR title ILIKE ?) ORDER BY updated_at DESC LIMIT 12`)}`)
                .bind(...args,...purchase.args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...service.args,...work.args, ...modules, term, term2).all();

            });
        },
        readSearchReferencesPeople(...args) { return RecordsIdentityAccessOwnedFacts(control).readSearchReferencesPeople(...args); },
        readSearchReferencesAssignment(...args) { return RecordsIdentityAccessFacts(database).readSearchReferencesAssignment(...args); },
        /** Retired: the reference-count guard executes in the native Rust records command route. */
        readAddReferenceCount() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records reference reader is unavailable.'); },
        /** Retired: record references execute in the native Rust records command route. */
        stageAddReferenceEffect() { throw new NativeApiError(503,'NATIVE_RECORDS_UNAVAILABLE','The native Records command is unavailable.'); },
    };
    return createRecordsApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, prepareControl: async () => { control = (await runtimeEnv()).CONTROL_DB; }, pageRecords: async (actor, moduleId, sectionId, query) => {
            return withExpenseWorkspaceRecordRead(database,actor,async database=>{
            if(moduleId==="finance")await requireFinanceWorkspaceHistory(database,actor);
            const scope = scopeSql(actor),basePeople=await peopleSourceReadScope(database,actor,"records"),commerce=await commerceSourceReadScope(database,actor,"records"),payroll=await payrollSourceReadScope(database,actor,"records"),count=await countSourceReadScope(database,actor,"records"),procurement=await procurementSourceReadScope(database,actor,"records"),transfer=await transferSourceReadScope(database,actor,"records"),projects=await projectSourceReadScope(database,actor,"records"),quality=await qualitySourceReadScope(database,actor,"records"),learning=await learningSourceReadScope(database,actor,"records"),assets=await assetSourceReadScope(database,actor,"records"),landedCost=await landedCostSourceReadScope(database,actor,"records"),manufacturing=await manufacturingSourceReadScope(database,actor,"records"),talent=await talentSourceReadScope(database,actor,"records"),legal=await legalSourceReadScope(database,actor,"records"),crm=await crmSourceReadScope(database,actor,"records"),marketing=await marketingSourceReadScope(database,actor,"records"),followups=await followupSourceReadScope(database,actor,"records"),workforce=await workforceSourceReadScope(database,actor,"records"),benefits=await benefitSourceReadScope(database,actor,"records"),service=await serviceSourceReadScope(database,actor,"records"),work=await workSourceReadScope(database,actor,"records");
            return readListPage<RecordRow & {
                page_key: string;
            }>(database, { sql: recordRowPageSql(` WHERE archived_at IS NULL AND module_id=? AND section_id=? AND ${scope.sql} AND ${commerce.where} AND ${payroll.where} AND ${basePeople.where} AND ${count.where} AND ${procurement.where} AND ${transfer.where} AND ${projects.where} AND ${quality.where} AND ${learning.where} AND ${assets.where} AND ${landedCost.where} AND ${manufacturing.where} AND ${talent.where} AND ${legal.where} AND ${crm.where} AND ${marketing.where} AND ${followups.where} AND ${workforce.where} AND ${benefits.where} AND ${service.where} AND ${work.where} AND strpos(lower(title||' '||COALESCE(description,'')),lower(?))>0`), args: [moduleId, sectionId, ...scope.args,...commerce.args,...payroll.args,...basePeople.args,...count.args,...procurement.args,...transfer.args,...projects.args,...quality.args,...learning.args,...assets.args,...landedCost.args,...manufacturing.args,...talent.args,...legal.args,...crm.args,...marketing.args,...followups.args,...workforce.args,...benefits.args,...service.args,...work.args, query.q ?? ''], cursor: query.cursor, descending: true, scope: [actor.companyId, actor.userId, scope, moduleId, sectionId, query.q ?? ''] });

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
/** Native context for a Records-owned workspace record. Never falls back to TS SQL. */
export async function recordContext(actor: WorkContext, moduleId: string, id: string) {
    if (!actor.authority || !actor.iam)
        throw new NativeApiError(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before retrying.');
    const raw = process.env.ERPX_NATIVE_RECORDS_CONFIG;
    if (raw === undefined)
        throw new NativeApiError(503, 'NATIVE_RECORD_CONTEXT_UNAVAILABLE', 'The native Records reader is unavailable.');
    try {
        const client = createNativeRecordContextClient(nativeRecordContextConfiguration(raw));
        const { headers: requestHeaders } = await import('next/headers');
        const scope = actor.scope;
        return await client.context(await requestHeaders(), {
            userId: actor.userId,
            holdingId: actor.holdingId,
            holdingSlug: actor.holdingSlug,
            companyId: actor.companyId,
            branchId: actor.branchId,
            departmentId: actor.departmentId,
            branchSlug: scope.kind === 'branch' || scope.kind === 'department' ? (scope.branchSlug ?? null) : null,
            departmentSlug: scope.kind === 'department' ? scope.departmentSlug : null,
            assignmentId: actor.assignmentId,
            role: actor.role,
            assignmentScope: actor.assignmentScope,
            modules: actor.modules,
            authority: actor.authority,
            iam: { permissions: actor.iam.permissions },
        }, { holdingSlug: actor.holdingSlug, moduleId, recordId: id });
    }
    catch (error) {
        const mapped = nativeRecordContextFailure(error);
        if (!mapped)
            throw error;
        throw new NativeApiError(mapped.status, mapped.code, mapped.message);
    }
}
export async function searchReferences(actor: WorkContext, query: string) { return invoke(() => application(undefined,actor).searchReferences(actor, query)); }
export async function addReference(actor: WorkContext, moduleId: string, id: string, input: z.infer<typeof addReferenceSchema>, key = crypto.randomUUID()) { return invoke(() => application(undefined,actor).addReference(actor, moduleId, id, input, key)); }
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
