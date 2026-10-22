import { z } from "zod";
import { serviceCategories } from "@erpx/feature-people_payroll/lib/employee-services";
const category = z.enum(serviceCategories.map(([id]) => id));
export const serviceTeamSchema = z.object({
    name: z.string().trim().min(2).max(160),
    active: z.boolean().default(true),
    expectedVersion: z.number().int().positive().default(1),
    members: z
        .array(z.object({
        userId: z.string().min(1).max(160),
        responsibility: z.enum(["lead", "member"]),
    }))
        .max(100),
    coverage: z
        .array(z.object({
        category,
        scopeKind: z.enum(["holding", "branch", "department"]),
        scopeNodeId: z.string().min(1).max(160),
    }))
        .min(1)
        .max(50),
});
export type ServiceTeam = {
    id: string;
    company_id: string;
    name: string;
    active: number;
    version: number;
    configuration_json: string;
};
export type ServiceTeamInput = z.infer<typeof serviceTeamSchema>;
export type ServiceHandler = {
    user_id: string;
    responsibility: "lead" | "member";
    name: string;
};
