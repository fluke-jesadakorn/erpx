import "server-only";
/** Closed, byte-identical read/lock SQL fragments shared by legal owner adapters.
 * Every call site prepares and executes its own statement; no result is cached. */
export const legalSettingsEnabledSql = "SELECT 1 FROM legal_settings WHERE company_id=? AND enabled";
export const legalDocumentsItemPairSql = "SELECT 1 FROM legal_documents WHERE id=? AND item_id=?";
export const legalDocumentByIdSql = "SELECT * FROM legal_documents WHERE id=?";
export const legalFilesAdvisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))";
export const legalCompanyAdvisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))";
