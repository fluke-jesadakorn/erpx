import "server-only";
import {closedProjection} from './closed-projection-source';
import {legalMembersWhereHead} from './legal-read-tail-r48-sources';

/** Closed, byte-identical legal_members read fragments shared by owner adapters.
 * No table, SQL, predicate or alias is accepted from a caller. */
export const legalMembersSnapshotSource = closedProjection(['item_id','user_id','access'], ', ', 'legal_members', ['FROM (', ') m']);
export const legalMembershipSql = legalMembersWhereHead('item_id=? AND user_id=?');
export const legalMembershipExistsSql = `EXISTS(${legalMembershipSql})`;
