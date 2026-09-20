import { z } from "zod";

const id = z.string().min(1).max(180).regex(/^[a-zA-Z0-9:_.-]+$/);
const version = z.number().int().positive();
const time = z.string().regex(/^([01]\d|2[0-3]):[0-5]\d$/, "Use a 24-hour time.");
const reason = z.string().trim().max(1000).default("");
export const rosterCommandSchema = z.discriminatedUnion("action", [
  z.object({ action: z.literal("template.save"), departmentId: id, id: id.optional(), expectedVersion: version.optional(), name: z.string().trim().min(1).max(80), startTime: time, endTime: time, capacity: z.number().int().min(1).max(100), active: z.boolean().default(true) }),
  z.object({ action: z.literal("week.create"), departmentId: id, weekStart: z.iso.date(), timezone: z.string().min(1).max(80).default("Asia/Bangkok"), copyWeekId: id.optional() }),
  z.object({ action: z.literal("week.publish"), id, expectedVersion: version }),
  z.object({ action: z.literal("shift.save"), weekId: id, expectedVersion: version, id: id.optional(), templateId: id.optional(), date: z.iso.date(), name: z.string().trim().min(1).max(80), startTime: time, endTime: time, capacity: z.number().int().min(1).max(100), personIds: z.array(id).max(100), reason }),
  z.object({ action: z.literal("shift.cancel"), id, expectedVersion: version, reason }),
  z.object({ action: z.literal("request.create"), kind: z.enum(["extra", "move", "swap"]), sourceAssignmentId: id.optional(), targetShiftId: id, targetAssignmentId: id.optional(), reason: z.string().trim().min(3).max(1000) }),
  z.object({ action: z.literal("request.decide"), id, expectedVersion: version, decision: z.enum(["accept", "approve", "reject", "withdraw"]), reason }),
]);
export type RosterCommand = z.infer<typeof rosterCommandSchema>;
export type RosterDepartment = { id: string; name: string; branch_id: string | null; branch_name: string | null };
export type ShiftTemplate = { id: string; department_id: string; name: string; start_time: string; end_time: string; capacity: number; active: boolean; version: number };
export type RosterWeek = { id: string; company_id: string; branch_id: string | null; department_id: string; week_start: string; timezone: string; state: "draft" | "published"; version: number };
export type RosterShift = { id: string; week_id: string; name: string; local_date: string; start_time: string; end_time: string; starts_at: number; ends_at: number; capacity: number; state: "active" | "cancelled"; version: number; template_id: string | null };
export type RosterAssignment = { id: string; shift_id: string; person_id: string; engagement_id: string; person_name: string; state: "assigned" | "released"; version: number };
export type RosterRequest = { id: string; company_id: string; department_id: string; kind: "extra" | "move" | "swap"; requester_person_id: string; coworker_person_id: string | null; requester_name: string; coworker_name: string | null; source_assignment_id: string | null; source_version: number | null; target_shift_id: string; target_assignment_id: string | null; target_version: number | null; state: "pending_coworker" | "pending_manager" | "approved" | "rejected" | "withdrawn" | "invalidated"; reason: string; decision_note: string; version: number; created_at: number; permittedActions: string[] };
export type RosterConflict = { shiftId: string; personId?: string; message: string };
export type RosterHistory = { id: string; record_id: string; action: string; actor_name: string; reason: string; created_at: number };
export type RosterData = {
  asOf: number;
  departments: RosterDepartment[]; departmentId: string | null; weekStart: string;
  week: RosterWeek | null; weeks: RosterWeek[]; templates: ShiftTemplate[];
  shifts: RosterShift[]; assignments: RosterAssignment[]; requests: RosterRequest[];
  people: { person_id: string; legal_name: string; engagement_id: string }[];
  history: RosterHistory[]; conflicts: RosterConflict[];
  selfPersonId: string | null; canManage: boolean; canApprove: boolean;
};
export function addDays(date: string, days: number) {
  return new Date(Date.parse(`${date}T12:00:00Z`) + days * 86400000).toISOString().slice(0, 10);
}
export function rosterMonday(date = new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Bangkok", year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date())) {
  const day = new Date(`${date}T12:00:00Z`).getUTCDay();
  return addDays(date, -(day + 6) % 7);
}
export function rosterPending(state: RosterRequest["state"]) { return state === "pending_coworker" || state === "pending_manager"; }
