import "server-only";
import {socialAiUsageProjection} from './read-tail-sql';
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  social_ai_usage: socialAiUsageProjection('"id","company_id","actor_id","month","state","input_tokens","output_tokens","lease_until","created_at"'),
} as const;
