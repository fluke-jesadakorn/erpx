import { executiveGenerationJobsSql } from './read-tail-sql';
export const agentExecutiveDatedUsage=()=>executiveGenerationJobsSql('company_id,created_at');
export const agentExecutiveDatedCount=()=>executiveGenerationJobsSql('count(*)',' WHERE company_id=? AND created_at>=?');
