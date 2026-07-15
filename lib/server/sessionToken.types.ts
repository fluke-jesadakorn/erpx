// ActionName alias. Re-exported here so apiGuard.ts doesn't have to depend on
// web-admin's @/lib/permissions (which is a UI-only module). Server-action
// callers that need real type narrowing should still import from
// '@/lib/permissions' on the web-admin side.

export type ActionName = string;