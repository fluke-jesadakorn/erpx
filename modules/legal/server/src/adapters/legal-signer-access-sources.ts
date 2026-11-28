import "server-only";
import {legalMembershipSql} from './legal-members-read-sources';
/** Closed, byte-identical signer-access SQL fragments shared by legal owner adapters.
 * No table, SQL, predicate or alias is accepted from a caller. */
export const legalSignerAccessPrefix = `${legalMembershipSql} UNION ALL SELECT 1 FROM legal_shares WHERE item_id=? AND `;
export const legalSignerAccessSuffix = " AND revoked_at IS NULL AND expires_at>?";
