// Pure owner contract: native compatibility helpers remain outside the core graph.
export { rosterCommandSchema, addDays, rosterMonday, rosterPending } from "@/lib/people-roster-domain";
export type { RosterCommand, RosterDepartment, ShiftTemplate, RosterWeek, RosterShift, RosterAssignment, RosterRequest, RosterConflict, RosterHistory, RosterData } from "@/lib/people-roster-domain";
