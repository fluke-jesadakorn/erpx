import "server-only";
import {legalByIdAndCompanySql} from './legal-read-tail-r28-sources';
import {legalItemOrderedSql} from './legal-read-tail-r35-sources';
/** Byte-identical read fragments shared by Legal owner adapters (round 18).
 * Text only: every call site still prepares and executes with its own predicate and binds. */
export const legalRetentionPolicySelectSql = legalByIdAndCompanySql('legal_retention_policies');
export const legalActorScopeProjectionSql = "SELECT ?::text company_id,?::text branch_id,?::text department_id";
export const legalEventsByItemSql = legalItemOrderedSql('*','legal_events','created_at');
