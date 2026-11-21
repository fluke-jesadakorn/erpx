import "server-only";
import { nativeClearCommandAssertions } from '@erpx/server-collaboration/adapters/native-command-assertions';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationAiWorkflowFacts(database: SqlDatabase) { return { stageRunEvaluationAiEffect3(key: string, companyId: string) { return nativeClearCommandAssertions(database, companyId, key); } }; }
