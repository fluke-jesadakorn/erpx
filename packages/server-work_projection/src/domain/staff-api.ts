export type StaffResource="staff"|"staff-tasks"|"projects"|"routine-templates"|"routine-schedules"|"approvals";
export type StaffRequestContext={params:Promise<{holdingSlug:string;path?:string[]}>};
