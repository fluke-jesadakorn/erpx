import {z} from 'zod';export const resourceSchema = z.object({
  expectedVersion: z.number().int().positive().optional(), branchId: z.string().min(1),
  code: z.string().trim().min(2).max(64), name: z.string().trim().min(2).max(160),
  dailyMinutes: z.number().int().min(1).max(1440),
  workWeekdays: z.array(z.number().int().min(0).max(6)).min(1).max(7).default([1, 2, 3, 4, 5]),
  status: z.enum(["active", "archived"]).default("active"),
});
export const calendarSchema = z.object({
  expectedVersion: z.number().int().nonnegative(), resourceVersion: z.number().int().positive(),
  workDate: z.string().date(), availableMinutes: z.number().int().min(0).max(1440),
  reason: z.string().trim().min(3).max(3000),
});
