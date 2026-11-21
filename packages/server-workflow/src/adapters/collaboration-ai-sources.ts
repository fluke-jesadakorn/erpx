import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeClearCommandAssertions } from '@erpx/server-collaboration/adapters/native-command-assertions';
export function collaborationAiSources(db: SqlDatabase) { return { clearReserveAssertions(id: string, companyId: string) { return nativeClearCommandAssertions(db, companyId, id); }, clearFinishAssertions(id: string, companyId: string) { return nativeClearCommandAssertions(db, companyId, id); } }; }
