import { z } from 'zod';
import { businessRoles, type BusinessRole } from '@/lib/business-roles';
import type { WorkContext } from './authorization';
type IdentityError = (status: number, code: string, message: string) => Error;
export const grantSchema = z
    .object({
    role: z.enum(Object.keys(businessRoles) as [
        BusinessRole,
        ...BusinessRole[]
    ]),
    scope: z.enum(["holding", "branch", "department"]),
    expiresAt: z.iso.date(),
})
    .refine((input) => input.role !== "it_admin" || input.scope === "holding", {
    message: "IT administrators require company scope.",
    path: ["scope"],
});
export const acceptInvitationSchema = z.object({
    token: z.string().min(32).max(128),
    password: z.string().min(12).max(128),
});
export type Invitation = {
    id: string;
    holding_id: string;
    company_id: string;
    person_id: string;
    email: string;
    name: string;
    role: string;
    scope_kind: "holding" | "branch" | "department";
    branch_id: string | null;
    department_id: string | null;
    created_by: string;
    assignment_id: string;
    expires_at: number;
    status: string;
};
export type PersonFact = Record<string, string | number | null>;
export type PersonGrant = z.infer<typeof grantSchema>;
export type InvitationAcceptance = z.infer<typeof acceptInvitationSchema>;
export function assertGrantScope(actor: WorkContext, input: PersonGrant, person: PersonFact, error: IdentityError) {
    if ((actor.assignmentScope === "branch" && input.scope === "holding") || (actor.assignmentScope === "department" && input.scope !== "department"))
        throw error(403, "GRANT_SCOPE_FORBIDDEN", "The role must stay within your HR assignment scope.");
    if ((input.scope === "branch" && !person.branch_id) || (input.scope === "department" && !person.department_id))
        throw error(422, "PLACEMENT_REQUIRED", "Set the staff placement before granting this scope.");
}
