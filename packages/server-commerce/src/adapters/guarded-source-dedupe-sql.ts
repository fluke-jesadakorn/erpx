/** Byte-identical Commerce guard/source text shared by the posting-setup read
 * tail. Text only: every call site still prepares and executes its own
 * statement in its supplied transaction and no query result is cached here. */
export const COMMERCE_POSTING_ABSENT_SQL = 'NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)';
export const COMMERCE_RECEIPT_GUARD_SQL = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
