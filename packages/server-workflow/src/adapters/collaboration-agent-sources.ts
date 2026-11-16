import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function collaborationAgentSources(db: SqlDatabase) { return { clearReserveAssertions(usageId: string) { return db.prepare("SELECT 1").bind(); }, clearFinishAssertions(usageId: string) { return db.prepare("SELECT 1").bind(); }, clearFailureAssertions(usageId: string) { return db.prepare("SELECT 1").bind(); } }; }
