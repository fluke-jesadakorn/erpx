import "server-only";
import {finalRootOwnerProjectionSources as identityAccessSources} from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import {legalSettingsWhereHead} from './legal-read-tail-r48-sources';
import {legalPointByIdSql} from './legal-read-tail-r32-sources';
import {legalDestructionGuardSql} from './legal-read-tail-r52-sources';
/** Closed, byte-identical read/lock SQL fragments shared by legal owner adapters.
 * Every call site prepares and executes its own statement; no result is cached. */
export const legalSettingsEnabledSql = legalSettingsWhereHead('company_id=? AND enabled');
export const legalManagerAvailableSql = `SELECT 1 FROM (${identityAccessSources.assignments}) assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review')`;
export const legalDestructionActiveSql = legalDestructionGuardSql('active');
export const legalDocumentsItemPairSql = "SELECT 1 FROM legal_documents WHERE id=? AND item_id=?";
export const legalDocumentByIdSql = legalPointByIdSql('*','legal_documents');
export const legalMembersAggSql = "(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM legal_members m WHERE m.item_id=i.id)";
export const legalFilesAdvisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))";
export const legalCompanyAdvisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))";
