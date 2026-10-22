import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function collaborationAiSources(db: SqlDatabase) { return { clearReserveAssertions(id: string) { return db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(id); }, clearFinishAssertions(id: string) { return db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(id); } }; }
