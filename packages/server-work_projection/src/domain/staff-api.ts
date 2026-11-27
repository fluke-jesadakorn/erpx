export type StaffResource="staff"|"staff-tasks"|"projects"|"approvals";
export type StaffRequestContext={params:Promise<{holdingSlug:string;path?:string[]}>};
