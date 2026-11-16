import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function collaborationAiSources(db: SqlDatabase) { return { clearReserveAssertions(id: string) { return db.prepare("SELECT 1").bind(); }, clearFinishAssertions(id: string) { return db.prepare("SELECT 1").bind(); } }; }
