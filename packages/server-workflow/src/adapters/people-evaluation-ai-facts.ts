import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationAiWorkflowFacts(database: SqlDatabase) { return { stageRunEvaluationAiEffect3(key: string) { return database.prepare("SELECT 1").bind(); } }; }
