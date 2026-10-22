import { z } from "zod";
export const legalPeopleQuery = z.object({ purpose: z.enum(["owner", "reviewer", "member", "signer"]).default("owner"), itemId: z.string().optional(), search: z.string().trim().max(100).default(""), role: z.string().max(100).optional(), branchId: z.string().optional(), departmentId: z.string().optional() });
export type LegalPeopleQuery = z.infer<typeof legalPeopleQuery>;
export type LegalPerson = { id: string; name: string; role: string };
