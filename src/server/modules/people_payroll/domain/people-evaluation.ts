import { z } from 'zod';
import { bandSchema, compensationMetrics, evaluationDate, evaluationMetrics, type AssessmentContent, type Compensation, type PayBasis, type RoleProfile } from "@/lib/people-evaluation";
export const reason = z.string().trim().min(3).max(4000), idSchema = z.string().min(1).max(180), version = z.number().int().positive();
export type Row = Record<string, unknown>;
export type AssessmentRow = {
    id: string;
    company_id: string;
    engagement_id: string;
    cycle_id: string;
    role_id: string;
    role_version: number;
    period_start: string;
    period_end: string;
    status: string;
    content_json: string;
    version: number;
    created_by: string;
    reviewed_by: string | null;
    confirmed_by: string | null;
    updated_at: number;
};
export type EvaluationEmployee = {
    id: string;
    personId: string;
    name: string;
    userId: string | null;
    positionId: string | null;
    levelId: string | null;
    jobTitle: string;
    branchId: string | null;
    departmentId: string | null;
    department: string | null;
    currency: string;
    startDate: string;
    endDate: string | null;
    version: number;
};
export type EvaluationRow = {
    key: string;
    employee: EvaluationEmployee;
    assessment: {
        id: string;
        version: number;
        cycleId: string;
        status: string;
        periodStart: string;
        periodEnd: string;
        content: AssessmentContent;
        createdBy: string;
        reviewedBy: string | null;
        confirmedBy: string | null;
    } | null;
    role: RoleProfile | null;
    roleRef: {
        id: string;
        version: number;
    } | null;
    metrics: ReturnType<typeof evaluationMetrics>;
    compensation?: {
        wage: Compensation | null;
        band: z.infer<typeof bandSchema> | null;
        basis: PayBasis | null;
        basisVersion: number | null;
        metrics: ReturnType<typeof compensationMetrics>;
    };
};
export function createEvaluationQuerySchema(year: number) { return z.object({ year: z.coerce.number().int().min(2020).max(2200).default(year), date: evaluationDate.optional(), cycleId: idSchema.optional(), departmentId: idSchema.optional(), family: z.string().max(160).optional(), levelId: idSchema.optional(), location: z.string().max(160).optional(), engagementId: idSchema.optional() }).strict(); }
export type EvaluationQuery = z.infer<ReturnType<typeof createEvaluationQuerySchema>>;
