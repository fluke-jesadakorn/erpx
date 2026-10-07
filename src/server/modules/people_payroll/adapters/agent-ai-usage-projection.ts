export const agentExpenseMonthlyCount=()=>"SELECT count(*) FROM expense_ai_usage WHERE company_id=? AND month=?";
