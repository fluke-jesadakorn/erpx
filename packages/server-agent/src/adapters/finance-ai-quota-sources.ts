import { finalRootOwnerProjectionSources as runtimeProjectionSources } from "@erpx/server-runtime/public/final-root-owner-projection-sources";
import { agentMonthlyUsageColsSql } from './agent-monthly-usage-source';
export const financeAiAgentUsageSql=`${agentMonthlyUsageColsSql} FROM ai_workspace_turns UNION ALL ${agentMonthlyUsageColsSql} FROM (${runtimeProjectionSources.executive_generation_jobs}) executive_generation_jobs`;
