import "server-only";
import { type ClearanceKind, type ClearanceReference } from "@/lib/people_payroll/retirement";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { PreparationTask } from "@/lib/people_payroll/people-lifecycle";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requireFlowAccess } from "@/server/modules/workflow/public/flow-access";
import { BusinessRuleError } from '../../shared';
import { retirementClearanceFacts } from '../../identity_access/public/retirement-clearance-facts';
import { clearanceDocumentSource } from './clearance-document-sources';
import { createRetirementClearanceApplication, type RetirementClearanceRepository } from '../application/retirement-clearance';
export { canCompleteClearance, canReadClearanceEvidence } from '../domain/retirement-clearance';
function application(database: SqlDatabase) {
    const repository: RetirementClearanceRepository = { eligibleOwner(...args) { return retirementClearanceFacts(database).eligibleOwner(...args); },
        tasks(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return database.prepare(`SELECT t.*,p.legal_name,c.status case_status,e.branch_id,e.department_id FROM people_preparation_tasks t JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id WHERE t.company_id=? AND t.clearance_type IS NOT NULL AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)`).bind(companyId, branchId, branchId2, departmentId, departmentId2).all<PreparationTask & {
                legal_name: string;
                case_status: string;
            }>();
        },
        caseRow(id: string, companyId: string) {
            return database.prepare(`SELECT c.id,c.status,c.effective_at,c.definition,p.legal_name FROM people_lifecycle_cases c JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id WHERE c.id=? AND c.company_id=?`).bind(id, companyId).first<{
                id: string;
                status: string;
                effective_at: number;
                definition: string;
                legal_name: string;
            }>();
        },
        caseTasks(id: string, companyId: string) { return database.prepare("SELECT * FROM people_preparation_tasks WHERE case_id=? AND company_id=? ORDER BY id").bind(id, companyId).all<PreparationTask>(); },
        finalPays(companyId: string, id: string) {
            return database.prepare(`SELECT fp.task_id,fp.id preparation_id,fp.run_id,fp.status,fp.version,r.pay_group_id,r.currency,r.period_start,r.period_end,r.pay_date,r.status run_status,r.version run_version
   FROM hr_payroll_final_pay_preparations fp JOIN hr_payroll_runs r ON r.id=fp.run_id
   WHERE fp.company_id=? AND fp.case_id=? ORDER BY fp.created_at DESC`).bind(companyId, id).all<{
                task_id: string;
                preparation_id: string;
                run_id: string;
                status: string;
                version: number;
                pay_group_id: string;
                currency: string;
                period_start: string;
                period_end: string;
                pay_date: string;
                run_status: string;
                run_version: number;
            }>();
        },
        finalPayOptions(companyId: string, id: string) {
            return database.prepare(`SELECT t.id task_id,e.pay_group_id,e.currency,COALESCE(e.end_date,c.definition->>'lastWorkingDate') last_working_date
   FROM people_preparation_tasks t JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
   WHERE t.company_id=? AND t.case_id=? AND t.clearance_type='payroll'`).bind(companyId, id).all<{
                task_id: string;
                pay_group_id: string | null;
                currency: string;
                last_working_date: string | null;
            }>();
        },
        source: (companyId, id, kind) => clearanceDocumentSource(database, companyId, id, kind),
        accessEnded(companyId: string, id: string, now: number) { return database.prepare("SELECT 1 FROM people_access_cutoffs WHERE company_id=? AND case_id=? AND cancelled_at IS NULL AND effective_at<=? AND restored_at IS NULL").bind(companyId, id, now).first(); },
        directoryEvidence(...args) { return retirementClearanceFacts(database).directoryEvidence(...args); },
        countryPacks() {
            return database.prepare("SELECT id,version,effective_from,effective_to FROM hr_country_packs WHERE country_code='TH' AND status='active' AND adapter_id='thai-payroll-v1' ORDER BY effective_from DESC,id").all<{
                id: string;
                version: string;
                effective_from: string;
                effective_to: string | null;
            }>();
        },
        owners(...args) { return retirementClearanceFacts(database).owners(...args); } };
    return createRetirementClearanceApplication({ repository, clock: { now: Date.now }, requireSourceAccess: async (actor, kind, id) => {
            try {
                return await requireFlowAccess(actor, kind, id);
            }
            catch (error) {
                if (error instanceof ApiError)
                    throw new BusinessRuleError(error.status, error.code, error.message);
                throw error;
            }
        } });
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function eligibleClearanceOwner(db: SqlDatabase, companyId: string, userId: string, kind: ClearanceKind) { return invoke(() => application(db).eligibleClearanceOwner(companyId, userId, kind)); }
export async function requireClearanceCompletion(db: SqlDatabase, actor: WorkContext, task: PreparationTask, input: {
    status?: string;
    reason?: string;
    references?: ClearanceReference[];
}) { return invoke(() => application(db).requireClearanceCompletion(actor, task, input)); }
export async function requireClearancePlan(db: SqlDatabase, actor: WorkContext, task: PreparationTask, assigneeId: string | null) { return invoke(() => application(db).requireClearancePlan(actor, task, assigneeId)); }
export async function retirementWork(db: SqlDatabase, actor: WorkContext) { return invoke(() => application(db).retirementWork(actor)); }
export async function clearanceCase(db: SqlDatabase, actor: WorkContext, id: string) { return invoke(() => application(db).clearanceCase(actor, id)); }
export async function clearanceOwners(db: SqlDatabase, actor: WorkContext) { return invoke(() => application(db).clearanceOwners(actor)); }
