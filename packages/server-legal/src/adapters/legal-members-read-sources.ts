import "server-only";
import {legalMembersWhereHead} from './legal-read-tail-r48-sources';

/** Closed, byte-identical legal_members read fragments shared by owner adapters.
 * No table, SQL, predicate or alias is accepted from a caller. */
export const legalMembersSnapshotSource = "FROM (SELECT \"item_id\", \"user_id\", \"access\" FROM legal_members) m";
export const legalMembershipSql = legalMembersWhereHead('item_id=? AND user_id=?');
export const legalMembershipExistsSql = `EXISTS(${legalMembershipSql})`;
