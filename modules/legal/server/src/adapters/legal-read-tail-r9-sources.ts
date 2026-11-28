import "server-only";
import {finalRootOwnerProjectionSources as identityAccessSources} from "@erpx/identity-access-server/public/final-root-owner-projection-sources";
import {legalSettingsWhereHead} from './legal-read-tail-r48-sources';
import {legalPointByIdSql} from './legal-read-tail-r32-sources';
import {legalDestructionGuardSql} from './legal-read-tail-r52-sources';
import {legalPointByIdAndKeySql} from './legal-read-tail-r34-sources';
import {legalJsonbAggByItemSql} from './legal-read-tail-r25-sources';
/** Closed, byte-identical read/lock SQL fragments shared by legal owner adapters.
 * Every call site prepares and executes its own statement; no result is cached. */
export const legalSettingsEnabledSql = legalSettingsWhereHead('company_id=? AND enabled');
export const legalManagerAvailableSql = `SELECT 1 FROM (${identityAccessSources.assignments}) assignments WHERE company_id=? AND role='legal_manager' AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND legal_person_eligible(company_id,user_id,?,?,'legal.review')`;
export const legalDestructionActiveSql = legalDestructionGuardSql('active');
export const legalDocumentsItemPairSql = legalPointByIdAndKeySql('1','legal_documents','item_id');
export const legalDocumentByIdSql = legalPointByIdSql('*','legal_documents');
export const legalMembersAggSql = legalJsonbAggByItemSql('m','legal_members','user_id');
export const legalFilesAdvisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))";
export const legalCompanyAdvisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))";
