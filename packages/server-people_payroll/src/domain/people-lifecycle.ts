import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { type AccessPackageDefinition } from "@erpx/feature-people_payroll/lib/people-lifecycle";
import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { BusinessRuleError } from '@erpx/server-shared/index';
export type LifecyclePositionFact = {
    id: string;
    company_id: string;
    version: number;
    title: string;
    requirements: unknown;
    branch_id: string | null;
    department_id: string | null;
    level_id: string;
    active: number;
};
export type LifecycleGrantFact = {
    id: string;
    definition: unknown;
    assignment_id: string | null;
};
export type LifecyclePackageRevisionFact = {
    definition: AccessPackageDefinition;
    package_id: string;
    revision: number;
};
export type LifecycleRoleFact = {
    published_revision: number;
    scopes_json: string[];
};
export type LifecyclePersonFact = {
    legal_name: string;
    id: string;
    version: number;
};
export function fail(message: string, code = "PEOPLE_CHANGED", status = 409): never {
    throw new BusinessRuleError(status, code, message);
}
export const parse = <T>(v: T | string): T => typeof v === "string" ? JSON.parse(v) : v;
export function company(actor: WorkContext) {
    if (actor.branchId || actor.departmentId)
        fail("Choose the Company workspace to prepare shared access.", "COMPANY_SCOPE_REQUIRED", 403);
}
export function readable(actor: WorkContext) {
    if (!["people.manage", "iam.read", "organization.manage"].some((p) => hasPermission(actor, p)))
        fail("People access is unavailable for this role.", "PERMISSION_REQUIRED", 403);
}
export const visible = (actor: WorkContext, row: {
    branch_id: string | null;
    department_id: string | null;
}) => (!actor.branchId || row.branch_id === actor.branchId) &&
    (!actor.departmentId || row.department_id === actor.departmentId);
export type LegacyCaseToSupersede = {
    id: string;
    version: number;
    status: string;
    kind: "onboarding" | "transfer";
    effectiveAt: number;
};
export const lifecycleSettingsInput = z.object({
    expectedVersion: z.number().int().nonnegative(),
    itAssigneeId: z.string().min(1).nullable(),
    onboardingTasks: z.array(z.string().min(2).max(300)).max(30),
    offboardingTasks: z.array(z.string().min(2).max(300)).max(30),
});
export const rehireInput = z.object({
    personId: z.string().min(1),
    engagementId: z.string().min(1),
    positionId: z.string().min(1),
    startDate: z.iso.date(),
    reason: z.string().min(3).max(1000),
});
