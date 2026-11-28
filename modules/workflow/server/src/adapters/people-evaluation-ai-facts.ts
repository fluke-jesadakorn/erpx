import "server-only";
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
export function peopleEvaluationAiWorkflowFacts(database: SqlDatabase) { return { stageRunEvaluationAiEffect3(key: string, companyId: string) { return nativeClearCommandAssertions(database, companyId, key); } }; }
