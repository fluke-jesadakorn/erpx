/** Byte-identical posting-prerequisite reference SQL shared by the Finance owner adapters.
 * Text only: every call site still prepares and executes with its own binds and order. */
export const POSTING_FINANCE_PERIODS_SQL = "SELECT p.id,p.name,p.start_date,p.end_date,p.state,p.close_version version,to_jsonb(p)::text source FROM finance_periods p WHERE p.company_id=? ORDER BY p.id";
export const POSTING_COMPANY_FX_RATES_SQL = "SELECT r.id,r.currency,r.effective_date,r.currency_per_usd::text,r.confirmed_at,to_jsonb(r)::text source FROM company_fx_rates r WHERE r.company_id=? ORDER BY r.id";
export const POSTING_COMPANY_MONEY_IMAGE_SQL = "SELECT 'settings' kind,to_jsonb(s)::text source FROM company_money_settings s WHERE s.company_id=? UNION ALL SELECT 'scale:'||s.currency kind,to_jsonb(s)::text source FROM company_currency_scales s WHERE s.company_id=? ORDER BY kind";
export const POSTING_COMMAND_RECEIPT_RECORD_SQL = "SELECT record_id FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
