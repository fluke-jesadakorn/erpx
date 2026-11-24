import "server-only";
import {finalRootOwnerProjectionSources as identityAccessSources} from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
/** Closed, byte-identical read/lock SQL fragments shared by legal owner adapters.
 * Every call site prepares and executes its own statement; no result is cached. */
export const legalSettingsEnabledSql = "SELECT 1 FROM legal_settings WHERE company_id=? AND enabled";
export const legalManagerAvailableSql = `SELECT 1 FROM (${identityAccessSources.assignments}) assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review')`;
export const legalDocumentsItemPairSql = "SELECT 1 FROM legal_documents WHERE id=? AND item_id=?";
export const legalDocumentByIdSql = "SELECT * FROM legal_documents WHERE id=?";
export const legalFilesAdvisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))";
export const legalCompanyAdvisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))";
