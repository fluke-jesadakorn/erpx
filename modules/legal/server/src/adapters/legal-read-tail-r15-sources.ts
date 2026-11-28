import "server-only";
import { legalPointByKeySql } from './legal-read-tail-r36-sources';
/** Byte-identical later-approval guard SQL shared by Legal executed-copy assertions.
 * R63: the fixed point-read head delegates to the verified r36 arbitrary-key
 * renderer with the fixed 'item_id' key; only the trusted `cycle>?` tail stays
 * local. Text only: every assertion site still prepares and executes with its
 * own two binds, and the rendered guard stays byte-identical. */
export const legalLaterApprovalGuardSql =
  `NOT EXISTS(${legalPointByKeySql('1', 'legal_approvals', 'item_id')} AND cycle>?)`;
