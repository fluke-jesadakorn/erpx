import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { type Row } from '../../people_payroll/public/people-evaluation-contract';
export function peopleEvaluationProjectsFacts(database: SqlDatabase) {
    return { readEvaluationEvidenceSourcesTasksprojectslearning2(companyId: string, userId: string | null, sourceId: string | null, sourceId2: string | null, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT p.id,p.title,p.version,p.updated_at FROM staff_projects p JOIN staff_project_members m ON m.project_id=p.id WHERE p.company_id=? AND m.user_id=? AND (?::text IS NULL OR p.id=?) AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?) ORDER BY p.updated_at DESC LIMIT 100").bind(companyId, userId, sourceId, sourceId2, branchId, branchId2, departmentId, departmentId2).all<Row>(); } };
}
