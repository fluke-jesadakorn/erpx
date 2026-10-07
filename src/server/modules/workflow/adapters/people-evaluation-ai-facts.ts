import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function peopleEvaluationAiWorkflowFacts(database: SqlDatabase) { return { stageRunEvaluationAiEffect3(key: string) { return database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key); } }; }
