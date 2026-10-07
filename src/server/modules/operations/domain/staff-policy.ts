import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { BusinessRuleError } from '../../shared';
export type StaffActor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;
export type RecordScope = {
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
};
export type StaffPerson = RecordScope & {
    person_id: string;
    user_id: string;
    legal_name: string;
    engagement_id: string;
    engagement_version: number;
    worker_type: "employee" | "contractor";
    start_date: string;
    end_date: string | null;
    manager_person_id: string | null;
    level_id: string | null;
    level_name: string | null;
    level_rank: number | null;
    level_number?: number | null;
    position_id?: string | null;
    roster_required?: number | null;
    is_ceo?: number | null;
    self_service_top_level?: boolean;
};
export function scopeActor<T extends StaffActor>(actor: T, record: RecordScope): T {
    return {
        ...actor,
        companyId: record.company_id,
        branchId: record.branch_id,
        departmentId: record.department_id,
    };
}
export type RouteStep = {
    userId: string;
    authority: "named" | "department" | "hr_admin" | "holding_owner";
    candidateUserIds?: string[];
    requiredRole?: string;
    stage?: string;
    snapshot?: Record<string, unknown>;
};
export type ApprovalRoute = {
    steps: RouteStep[];
    blockedReason: string | null;
    scope: RecordScope;
    checks?: {
        query: string;
        args: (string | number | null)[];
    }[];
};
export function reportingChain(people: StaffPerson[], personId: string): StaffPerson[] {
    const chain: StaffPerson[] = [];
    const seen = new Set([personId]);
    let next = people.find((p) => p.person_id === personId)?.manager_person_id;
    while (next) {
        if (seen.has(next))
            throw new BusinessRuleError(409, "REPORTING_CYCLE", "A reporting cycle must be corrected before submission.");
        seen.add(next);
        const matches = people.filter((p) => p.person_id === next);
        if (matches.length !== 1)
            throw new BusinessRuleError(409, "MANAGER_UNAVAILABLE", "The reporting manager needs one active engagement and account.");
        chain.push(matches[0]);
        next = matches[0].manager_person_id;
        if (chain.length > 100)
            throw new BusinessRuleError(409, "REPORTING_CYCLE", "The reporting chain is too deep to resolve safely.");
    }
    return chain;
}
