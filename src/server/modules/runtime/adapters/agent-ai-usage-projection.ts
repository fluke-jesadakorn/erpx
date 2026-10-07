export const agentExecutiveDatedUsage=()=>"SELECT company_id,created_at FROM executive_generation_jobs";
export const agentExecutiveDatedCount=()=>"SELECT count(*) FROM executive_generation_jobs WHERE company_id=? AND created_at>=?";
