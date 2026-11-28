import {agentFinanceMonthlyUsage} from '@erpx/finance-server/public/agent-ai-usage-projection';
import {agentSocialMonthlyCount} from '@erpx/collaboration-server/public/agent-ai-usage-projection';
import {agentExpenseMonthlyCount} from '@erpx/people-payroll-server/public/agent-ai-usage-projection';
import {agentRequestMonthlyUsage} from '@erpx/records-server/public/agent-ai-usage-projection';
import {agentExecutiveDatedCount} from '@erpx/platform-runtime-server/public/agent-ai-usage-projection';
import {financeAiCompanyAllowanceSql} from '@erpx/manage-erpx-server/public/finance-ai-quota-sources';
import {agentTurnCountGuardSql} from './operations-guided-intake-facts';
import {agentMonthlyUnionCountSql} from './read-tail-r25-source-sql';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
/** Byte-identical two-branch request/finance union count, now composed from the
 * one Agent monthly-union count head; the `calls` alias is the only variation. */
const executiveQuotaUnionCountSql = agentMonthlyUnionCountSql(`${agentRequestMonthlyUsage()} UNION ALL ${agentFinanceMonthlyUsage()}`, 'calls');
export function executiveGenerationQuota(database:SqlDatabase){return {guard7(key:string,value:"executive-quota",companyId:string,start:number,companyId2:string,start2:number,companyId3:string,month:string,companyId4:string,month2:string,companyId5:string,month3:string,companyId6:string){return assertion(database,key,value,`(${agentExecutiveDatedCount()})+${agentTurnCountGuardSql('>=')}+(${agentSocialMonthlyCount()})+(${agentExpenseMonthlyCount()})+${executiveQuotaUnionCountSql}<${financeAiCompanyAllowanceSql}`,[companyId,start,companyId2,start2,companyId3,month,companyId4,month2,companyId5,month3,companyId6]);}};}