import {z} from 'zod';import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";import type {WorkContext} from '@erpx/server-identity_access/public/authorization';export const directorySchema = z.object({
  legalName: z.string().trim().min(2).max(200),
  preferredName: z.string().trim().max(160).default(""),
  email: z.email().transform((v) => v.toLowerCase()),
  phone: z.string().trim().max(60).default(""),
  workEmail: z.union([z.email().transform((v) => v.toLowerCase()), z.literal("")]).optional(),
  workPhone: z.string().trim().max(60).optional(),
  countryCode: z.string().regex(/^[A-Z]{2}$/),
  branchId: z.string().min(1),
  departmentId: z.string().nullable().default(null),
  jobTitle: z.string().trim().min(2).max(160),
  workerType: z.enum(["employee", "contractor"]),
  employmentType: z.enum(["salaried", "daily", "hourly", "shift", "contractor"]),
  startDate: z.iso.date(),
  currency: z.enum(["USD", "EUR", "GBP", "THB", "SGD", "CAD"]),
  managerPersonId: z.string().nullable().default(null),
  levelId: z.string().min(1).nullable().default(null),
  positionId: z.string().min(1).nullable().optional(),
  acceptRecommendedPay: z.boolean().default(false),
  acceptRecommendedLeave: z.boolean().default(false),
  reason: z.string().trim().min(3).max(1000).default("Staff profile updated"),
  linkedMemberUserId: z.string().max(160).optional(),
});
export const directoryUpdateSchema = directorySchema.extend({
  expectedVersion: z.number().int().positive(),
  engagementId: z.string().min(1).optional(),
  expectedEngagementVersion: z.number().int().positive().optional(),
});
export const directoryStatusSchema = z.object({
  status: z.enum(["active", "inactive", "archived"]),
  expectedVersion: z.number().int().positive(),
});
export function canManagePeople(actor: WorkContext) {
  return hasPermission(actor,"people.manage");
}
export function canReadCompensation(actor: WorkContext, userId: unknown) {
  return (
    hasPermission(actor,"people.compensation.read") ||
    hasPermission(actor,"people.compensation.payroll") ||
    userId === actor.userId
  );
}
