import 'server-only';
import { requestAiUsageProjectionSql } from './ai-usage-projection';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function workflowFinalOwnerProjections(){return {
  "finance_ai_request_sources_request_ai_usage_from": `FROM (${requestAiUsageProjectionSql}) request_ai_usage`
} as const;}
