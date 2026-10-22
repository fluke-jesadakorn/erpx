import { sensitiveAttachmentPolicies, isSensitiveAttachmentTarget } from "@erpx/server-files/domain/attachments/sensitive-policy";
import type { ClearanceKind } from "@erpx/feature-people_payroll/lib/retirement";
import type { AssessmentRow, EvaluationEmployee, EvaluationDefinition } from "@erpx/server-people_payroll/public/attachments-evaluation-contracts";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { AttachmentOperation, AttachmentPolicy, AttachmentTargetScope } from "@erpx/server-files/domain/attachments/types";
export type AttachmentRows<Row> = {
    results: Row[];
};
export type SensitiveAttachmentUnit<Effect> = {
    evaluationAuthority(actor: WorkContext, permission: string): Promise<WorkContext>;
    evaluationAssessment(actor: WorkContext, id: string): Promise<{
        row: AssessmentRow;
        employee: EvaluationEmployee;
    }>;
    evaluationDefinition(actor: WorkContext, id: string, kind: "cycle"): Promise<EvaluationDefinition>;
    readHrPeople1(facts: {
        userId: string;
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        user_id: string | null;
        version: number;
        branch_id: string | null;
        department_id: string | null;
        is_self: number;
    } | null>;
    readPeopleLifecycleCases1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        branch_id: string | null;
        department_id: string | null;
    } | null>;
    readPeoplePreparationTasks1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        status: string;
        assignee_id: string | null;
        completed_by: string | null;
        clearance_type: ClearanceKind;
        branch_id: string | null;
        department_id: string | null;
    } | null>;
    readHrPayrollRuns1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        branch_id: null;
        department_id: null;
        version: number;
    } | null>;
    readHrPayrollAdvances1(facts: {
        userId: string;
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        status: string;
        requested_by: string;
        branch_id: string | null;
        department_id: string | null;
        is_self: number;
    } | null>;
    readHrPayrollFinalPayPreparations1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        status: string;
        assignee_id: string | null;
        completed_by: string | null;
        clearance_type: ClearanceKind;
        branch_id: string | null;
        department_id: string | null;
    } | null>;
    readTalentApplications1(facts: {
        userId: string;
        userId2: string;
        userId3: string;
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        owner_branch_id: string | null;
        owner_department_id: string | null;
        is_owner: number;
    } | null>;
    readTalentOffers1(facts: {
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        branch_id: string | null;
        department_id: string | null;
        owner_id: string;
        pending_hire_delivery: number;
    } | null>;
    readLearningAssignments1(facts: {
        userId: string;
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        branch_id: null;
        department_id: null;
        status: string;
        is_learner: number;
    } | null>;
    readLearningAssignments2(facts: {
        userId: string;
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        branch_id: string | null;
        department_id: string | null;
        status: string;
        learner_id: string;
        is_learner: number;
    } | null>;
    readHrPeople2(facts: {
        userId: string;
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        branch_id: null;
        department_id: null;
        status: string;
        is_employee: number;
    } | null>;
    readBenefitDependents1(facts: {
        userId: string;
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        status: string;
        branch_id: string | null;
        department_id: string | null;
        is_owner: number;
    } | null>;
    readBenefitElections1(facts: {
        userId: string;
        targetId: string;
        companyId: string;
    }): Promise<{
        company_id: string;
        version: number;
        status: string;
        branch_id: string | null;
        department_id: string | null;
        is_owner: number;
    } | null>;
    versionGuard(commandId: string, targetType: string, targetId: string, companyId: string, version: number, index: number): Effect | null;
    versionStep(targetType: string, targetId: string, expectedVersion: number, now: number): Effect | null;
};
export function createSensitiveAttachmentPolicy<Effect>(ports: {
    clock: Clock;
    error(status: number, code: string, message: string): Error;
    canCompleteClearance(arg0: WorkContext, arg1: {
        company_id: string;
        version: number;
        status: string;
        assignee_id: string | null;
        completed_by: string | null;
        clearance_type: ClearanceKind;
        branch_id: string | null;
        department_id: string | null;
    }): boolean;
    canReadClearanceEvidence(arg0: WorkContext, arg1: {
        company_id: string;
        version: number;
        status: string;
        assignee_id: string | null;
        completed_by: string | null;
        clearance_type: ClearanceKind;
        branch_id: string | null;
        department_id: string | null;
    }): boolean;
}) {
    function privateNotFound(): never {
        throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "The file owner was not found in this workspace.");
    }
    function inScope(actor: WorkContext, row: {
        branch_id: string | null;
        department_id: string | null;
    }) {
        return ((!actor.branchId || actor.branchId === row.branch_id) &&
            (!actor.departmentId || actor.departmentId === row.department_id));
    }
    function scope(row: {
        company_id: string;
        branch_id: string | null;
        department_id: string | null;
        version: number;
    }, editable: boolean): AttachmentTargetScope {
        return {
            companyId: row.company_id,
            branchId: row.branch_id,
            departmentId: row.department_id,
            version: Number(row.version),
            editable,
        };
    }
    const employeeSelfVisible = new Set([
        "employment_contract",
        "employment_amendment",
        "professional_certificate",
    ]);
    async function employeePerson(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation) {
        const row = await db.readHrPeople1({ userId: actor.userId, targetId: targetId, companyId: actor.companyId });
        if (!row || !inScope(actor, row))
            privateNotFound();
        const self = Number(row.is_self) > 0;
        const read = hasPermission(actor, "people.documents.read") ||
            (self && employeeSelfVisible.has(policy.purposeKey));
        const write = hasPermission(actor, "people.documents.manage") ||
            (self && policy.purposeKey === "professional_certificate");
        if (operation === "read" ? !read : !write)
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function lifecycleCase(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readPeopleLifecycleCases1({ targetId: targetId, companyId: actor.companyId });
        const permission = operation === "read" ? "people.documents.read" : "people.documents.manage";
        if (!row || !inScope(actor, row) || !hasPermission(actor, permission))
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function clearanceTask(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readPeoplePreparationTasks1({ targetId: targetId, companyId: actor.companyId });
        if (!row || !inScope(actor, row))
            privateNotFound();
        const allowed = operation === "read"
            ? ports.canReadClearanceEvidence(actor, row)
            : row.status === "pending" && ports.canCompleteClearance(actor, row);
        if (!allowed)
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function payrollRun(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readHrPayrollRuns1({ targetId: targetId, companyId: actor.companyId });
        const permission = operation === "read" ? "payroll.documents.read" : "payroll.documents.manage";
        if (!row ||
            actor.branchId ||
            actor.departmentId ||
            !hasPermission(actor, permission))
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function payrollAdvance(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readHrPayrollAdvances1({ userId: actor.userId, targetId: targetId, companyId: actor.companyId });
        if (!row || !inScope(actor, row))
            privateNotFound();
        const self = Number(row.is_self) > 0;
        const read = hasPermission(actor, "payroll.documents.read") || self;
        const write = hasPermission(actor, "payroll.documents.manage") ||
            (self && row.status === "pending");
        if (operation === "read" ? !read : !write)
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function finalPay(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readHrPayrollFinalPayPreparations1({ targetId: targetId, companyId: actor.companyId });
        if (!row || !inScope(actor, row))
            privateNotFound();
        const allowed = operation === "read"
            ? hasPermission(actor, "payroll.documents.read") &&
                ports.canReadClearanceEvidence(actor, row)
            : hasPermission(actor, "payroll.documents.manage") &&
                row.status === "prepared" &&
                ports.canCompleteClearance(actor, row);
        if (!allowed)
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function talentCandidate(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readTalentApplications1({ userId: actor.userId, userId2: actor.userId, userId3: actor.userId, targetId: targetId, companyId: actor.companyId });
        if (!row)
            privateNotFound();
        const scoped = {
            ...row,
            branch_id: row.owner_branch_id,
            department_id: row.owner_department_id,
        };
        const canManage = hasPermission(actor, "talent.documents.manage");
        const canRead = hasPermission(actor, "talent.documents.read") &&
            (canManage || Number(row.is_owner) > 0);
        if (!inScope(actor, scoped) || (operation === "read" ? !canRead : !canManage))
            privateNotFound();
        return scope(scoped, operation !== "read");
    }
    async function talentOffer(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readTalentOffers1({ targetId: targetId, companyId: actor.companyId });
        if (!row || !inScope(actor, row))
            privateNotFound();
        const canManage = hasPermission(actor, "talent.documents.manage");
        const canRead = hasPermission(actor, "talent.documents.read") &&
            (canManage || row.owner_id === actor.userId);
        if (operation === "read" ? !canRead : !canManage)
            privateNotFound();
        if (operation !== "read" && Number(row.pending_hire_delivery)>0)
            throw ports.error(409,"HIRE_HANDOFF_PENDING","Complete the original HR hire delivery before changing accepted-offer files.");
        return scope(row, operation !== "read");
    }
    async function learningCourse(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readLearningAssignments1({ userId: actor.userId, targetId: targetId, companyId: actor.companyId });
        if (!row)
            privateNotFound();
        const read = hasPermission(actor, "learning.documents.read") ||
            (Number(row.is_learner) > 0 && ["published", "retired"].includes(row.status));
        const write = hasPermission(actor, "learning.documents.manage");
        if (operation === "read" ? !read : !write)
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function learningAssignment(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readLearningAssignments2({ userId: actor.userId, targetId: targetId, companyId: actor.companyId });
        if (!row || !inScope(actor, row))
            privateNotFound();
        const self = Number(row.is_learner) > 0;
        const read = hasPermission(actor, "learning.documents.read") || self;
        const write = hasPermission(actor, "learning.documents.manage") ||
            (self && ["in_progress", "completed"].includes(row.status));
        if (operation === "read" ? !read : !write)
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function benefitPlan(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readHrPeople2({ userId: actor.userId, targetId: targetId, companyId: actor.companyId });
        if (!row)
            privateNotFound();
        const read = hasPermission(actor, "benefits.documents.read") ||
            (Number(row.is_employee) > 0 && ["published", "retired"].includes(row.status));
        const write = hasPermission(actor, "benefits.documents.manage");
        if (operation === "read" ? !read : !write)
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function benefitDependent(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, targetId: string, operation: AttachmentOperation) {
        const row = await db.readBenefitDependents1({ userId: actor.userId, targetId: targetId, companyId: actor.companyId });
        if (!row || !inScope(actor, row))
            privateNotFound();
        const self = Number(row.is_owner) > 0;
        const read = hasPermission(actor, "benefits.documents.read") || self;
        const write = hasPermission(actor, "benefits.documents.manage") ||
            (self && row.status === "pending");
        if (operation === "read" ? !read : !write)
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function benefitElection(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation) {
        const row = await db.readBenefitElections1({ userId: actor.userId, targetId: targetId, companyId: actor.companyId });
        if (!row || !inScope(actor, row))
            privateNotFound();
        const self = Number(row.is_owner) > 0;
        const managerRead = hasPermission(actor, "benefits.documents.read");
        const managerWrite = hasPermission(actor, "benefits.documents.manage");
        const selfRead = self &&
            (policy.purposeKey === "election_document" ||
                (policy.purposeKey === "provider_proof" &&
                    ["acknowledged", "released"].includes(row.status)));
        const selfWrite = self &&
            policy.purposeKey === "election_document" &&
            ["elected", "submitted"].includes(row.status);
        if (operation === "read" ? !managerRead && !selfRead : !managerWrite && !selfWrite)
            privateNotFound();
        return scope(row, operation !== "read");
    }
    async function authorizeSensitiveAttachmentTarget(db: SensitiveAttachmentUnit<Effect>, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope | null> {
        if (!isSensitiveAttachmentTarget(policy.targetType))
            return null;
        switch (policy.targetType) {
            case "hr_person":
                return employeePerson(db, actor, policy, targetId, operation);
            case "hr_evaluation_assessment": {
                actor = await db.evaluationAuthority(actor, operation === "read" ? "people.read" : "people.manage");
                if (!hasPermission(actor, "people.compensation.read") || !hasPermission(actor, operation === "read" ? "people.documents.read" : "people.documents.manage"))
                    privateNotFound();
                const { row, employee } = await db.evaluationAssessment(actor, targetId);
                const cycle = await db.evaluationDefinition(actor, row.cycle_id, "cycle");
                if (operation !== "read" && (!["draft", "calibrating"].includes(row.status) || cycle.data.status !== "open"))
                    privateNotFound();
                return { companyId: actor.companyId, branchId: employee.branchId, departmentId: employee.departmentId, version: row.version, editable: operation !== "read" };
            }
            case "people_lifecycle_case":
                return lifecycleCase(db, actor, targetId, operation);
            case "people_clearance_task":
                return clearanceTask(db, actor, targetId, operation);
            case "payroll_run":
                return payrollRun(db, actor, targetId, operation);
            case "payroll_advance":
                return payrollAdvance(db, actor, targetId, operation);
            case "payroll_final_pay":
                return finalPay(db, actor, targetId, operation);
            case "talent_candidate":
                return talentCandidate(db, actor, targetId, operation);
            case "talent_offer":
                return talentOffer(db, actor, targetId, operation);
            case "learning_course":
                return learningCourse(db, actor, targetId, operation);
            case "learning_assignment":
                return learningAssignment(db, actor, targetId, operation);
            case "benefit_plan":
                return benefitPlan(db, actor, targetId, operation);
            case "benefit_dependent":
                return benefitDependent(db, actor, targetId, operation);
            case "benefit_election":
                return benefitElection(db, actor, policy, targetId, operation);
            default:
                return null;
        }
    }
    function sensitiveAttachmentTargetVersionGuard(db: SensitiveAttachmentUnit<Effect>, commandId: string, targetType: string, targetId: string, companyId: string, version: number, index: number): Effect | null { return db.versionGuard(commandId, targetType, targetId, companyId, version, index); }
    function sensitiveAttachmentRecordVersionStep(db: SensitiveAttachmentUnit<Effect>, targetType: string, targetId: string, expectedVersion: number, now = ports.clock.now()): Effect | null { return db.versionStep(targetType, targetId, expectedVersion, now); }
    return { sensitiveAttachmentPolicies, isSensitiveAttachmentTarget, authorizeSensitiveAttachmentTarget, sensitiveAttachmentTargetVersionGuard, sensitiveAttachmentRecordVersionStep };
}
