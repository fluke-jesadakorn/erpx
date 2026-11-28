import "server-only";
import { requestAiUsageProjectionSql } from "./ai-usage-projection";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  request_ai_usage: requestAiUsageProjectionSql,
} as const;
