/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { templateSchema, scheduleSchema, listTemplates, getTemplate, canEditTemplate, saveTemplate, saveSchedule, changeScheduleState, endSchedule, runTemplate, generateOccurrences } from "@/server/modules/operations/adapters/routines";
export type { TemplateRow, ScheduleRow } from "@/server/modules/operations/adapters/routines";
