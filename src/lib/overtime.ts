import { z } from "zod";
const localTime = z.string().regex(/^\d{4}-\d{2}-\d{2}T(?:[01]\d|2[0-3]):[0-5]\d$/, "Enter a valid date and time.").refine(value=>z.iso.date().safeParse(value.slice(0,10)).success,"Enter a valid date.");
export const overtimeDetailsSchema = z.object({
  mode: z.enum(["advance", "retrospective"]),
  startLocal: localTime, endLocal: localTime,
  timezone: z.string().min(1).max(80).refine(value => { try { new Intl.DateTimeFormat("en", { timeZone: value }); return true; } catch { return false; } }, "Choose a valid timezone."),
  breakMinutes: z.number().int().min(0).max(1439).default(0),
  reason: z.string().trim().min(3).max(2048),
  lateExplanation: z.string().trim().max(2048).default(""),
  rosterShiftId: z.string().max(180).nullable().default(null),
});
export const overtimeCommandSchema = z.object({
  action: z.enum(["save", "submit", "authorize", "actuals", "verify", "approve", "return", "reject", "withdraw"]),
  expectedVersion: z.number().int().positive(),
  details: overtimeDetailsSchema.optional(),
  note: z.string().trim().max(2048).default(""),
});
export type OvertimeDetails = z.infer<typeof overtimeDetailsSchema>;
export type OvertimeCommand = z.infer<typeof overtimeCommandSchema>;
export const overtimeLabels: Record<string, string> = {
  draft: "Draft", pending_authorization: "Manager authorization", authorized: "Submit actual hours",
  pending_verification: "Manager verification", pending_hr: "HR approval", returned: "Correct actual hours",
  rejected: "Rejected", withdrawn: "Withdrawn", approved: "Approved · payroll handoff ready",
};
export function overtimeMinutes(startsAt: number, endsAt: number, unpaidBreak: number) {
  const elapsed = (endsAt - startsAt) / 60_000;
  if (!Number.isInteger(elapsed) || elapsed <= 0 || elapsed > 1440 || unpaidBreak >= elapsed)
    throw new Error("Enter an interval of up to 24 hours and a break shorter than the interval.");
  return elapsed - unpaidBreak;
}
