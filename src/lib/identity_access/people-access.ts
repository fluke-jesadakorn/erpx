import { z } from "zod";

const id = z.string().min(1).max(180);
const version = z.number().int().nonnegative().default(0);
const reason = z.string().trim().min(3).max(1000);
export const peopleAccessChange = z.discriminatedUnion("operation", [
  z.object({ operation: z.literal("organization"), id: id.optional(), expectedVersion: version, name: z.string().trim().min(2).max(150), kind: z.enum(["organization", "independent"]), reason }),
  z.object({ operation: z.literal("team"), id: id.optional(), expectedVersion: version, name: z.string().trim().min(2).max(150), organizationId: id, departmentId: id.nullable(), reason }),
  z.object({ operation: z.literal("subject"), id: id.optional(), expectedVersion: version, name: z.string().trim().min(2).max(150), email: z.email(), organizationId: id, teamId: id.nullable(), sponsorPersonId: id.nullable(), expiresAt: z.number().int().positive().nullable(), status: z.enum(["active", "suspended"]), reason }),
  z.object({ operation: z.literal("role"), id: id.optional(), expectedVersion: version, name: z.string().trim().min(2).max(100), permissions: z.array(id).min(1).max(1000), scopes: z.array(z.enum(["holding", "branch", "department"])).min(1), reason }),
  z.object({ operation: z.literal("grant"), subjectId: id, roleId: id, scopeKind: z.enum(["holding", "branch", "department"]), scopeNodeId: id.nullable(), validUntil: z.number().int().positive(), reason }),
  z.object({ operation: z.literal("invite"), subjectId: id, roleId: id, scopeKind: z.enum(["holding", "branch", "department"]), scopeNodeId: id.nullable(), validUntil: z.number().int().positive(), reason }),
  z.object({ operation: z.literal("revoke"), assignmentId: id, expectedVersion: z.number().int().positive(), reason }),
]);
export type PeopleAccessChange = z.infer<typeof peopleAccessChange>;
export type AccessOrganization = { id: string; name: string; kind: "organization" | "independent"; version: number };
export type ExternalTeam = { id: string; name: string; organizationId: string; departmentId: string | null; version: number };
export type AccessSubject = { id: string; name: string; email: string; kind: "internal" | "external"; userId: string | null; personId: string | null; departmentId: string | null; organizationId: string | null; teamId: string | null; sponsorPersonId: string | null; expiresAt: number | null; status: string; version: number };
export type AccessRole = { id: string; name: string; revision: number; permissions: string[]; grantablePermissions: string[]; scopes: string[]; builtIn: boolean };
export type PeopleAccessSnapshot = {
  subjects: AccessSubject[]; organizations: AccessOrganization[]; teams: ExternalTeam[];
  units: { id: string; name: string; kind: string; branchId: string | null }[];
  roles: AccessRole[]; permissions: { id: string; label: string }[];
  assignments: { id: string; userId: string; roleId: string; scopeKind: string; scopeNodeId: string | null; validUntil: number; version: number; active: boolean }[];
  canApply: boolean; authorizingUntil: number;
};
export type PeopleAccessPreview = { token: string; change: PeopleAccessChange; summary: string; affected: string[]; expiresAt: number };
