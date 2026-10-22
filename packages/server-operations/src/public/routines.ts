/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { templateSchema, scheduleSchema, listTemplates, getTemplate, canEditTemplate, saveTemplate, saveSchedule, changeScheduleState, endSchedule, runTemplate, generateOccurrences } from "@erpx/server-operations/adapters/routines";
export type { TemplateRow, ScheduleRow } from "@erpx/server-operations/adapters/routines";
