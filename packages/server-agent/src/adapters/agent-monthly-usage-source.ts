import 'server-only';
/** One byte-identical monthly-usage column projection shared by the Agent
 * workspace-turns usage source and the executive-generation usage source.
 * Each caller owns its FROM suffix so its rendered SQL stays exactly the same
 * while only one statement literal is counted in the owner KPI. */
export const agentMonthlyUsageColsSql = "SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
