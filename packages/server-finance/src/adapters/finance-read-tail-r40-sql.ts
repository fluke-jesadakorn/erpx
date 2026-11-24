/** G4 / retire.finance.read-tail-r40 exact and parameterized SQL families.
 *
 * Each export holds only SQL text shared by adapter sites. Callers keep their
 * own binds, result types and guards; nothing here is cached or executed.
 * Row-lock text, tax assertion guards and r15-r39 helpers are not changed.
 */
export const sourcePartyIdSql = (table: string) => `SELECT party_id FROM ${table} WHERE id=? AND company_id=?`;
export const commandReceiptReplayProbeSql = (operation: string) => `SELECT 1 FROM command_receipts WHERE command_id=? AND operation='${operation}' AND actor_id=? AND authority_json::jsonb->>'companyId'=?`;
export const ASSIGNMENT_VERSION_SQL = 'SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?';
export const POSTING_RECEIPT_IMAGE_SQL = "SELECT to_jsonb(r)::text image,r.record_id FROM command_receipts r WHERE r.command_id=? AND r.operation=? AND r.actor_id=? AND r.authority_json::jsonb->>'companyId'=?";
export const ACTIVE_ACCOUNT_CATEGORY_PROBE_SQL = "SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category=? AND status='active'";
export const ownedDocumentExistsSql = (table: string, predicate: string) => `SELECT 1 FROM ${table} d WHERE d.id=? AND d.company_id=? AND ${predicate}`;
export const sourcePartyStatusSql = (table: string) => `SELECT party_id,status FROM ${table} WHERE id=? AND company_id=?`;
export const bookAccountMappingImageSql = (alias: string) => `SELECT to_jsonb(m)::text ${alias} FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id WHERE r.company_id=? ORDER BY m.rule_id,m.primary_account_id`;
export const thaiDocumentRowsSql = (table: string) => `SELECT * FROM ${table} WHERE document_id=? AND company_id=? ORDER BY created_at,id`;
export const PAYMENT_FULL_ROW_SQL = 'SELECT * FROM finance_payments WHERE id=? AND company_id=?';
