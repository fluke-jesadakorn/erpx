import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';
export function collaborationAiSources(db: SqlDatabase) { return { clearReserveAssertions(id: string, companyId: string) { return nativeClearCommandAssertions(db, companyId, id); }, clearFinishAssertions(id: string, companyId: string) { return nativeClearCommandAssertions(db, companyId, id); } }; }
