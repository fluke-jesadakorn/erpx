import {workflowFinalOwnerProjections as recordsFinalOwnerProjections} from "../../records/public/workflow-final-owner-projections";
const recordsFinalSources=recordsFinalOwnerProjections();
export const financeAiRequestUsageSql=`SELECT company_id,month ${recordsFinalSources.finance_ai_request_sources_request_ai_usage_from}`;
