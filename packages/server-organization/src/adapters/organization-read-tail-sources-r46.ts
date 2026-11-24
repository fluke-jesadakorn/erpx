import 'server-only';

/** r46 additions: byte-identical read-tail SQL source. The helper renders
 * exactly the literal it replaces; callers keep their own binds, locks and
 * supplied statements. No r17-r45 helper, FOR SHARE/FOR UPDATE lock, LOCK TABLE
 * statement or spaced JOIN constant is changed. Only the owner-supplied action
 * literal and ordering tail vary, never caller input. */

/** Latest change-event snapshot for one action. Family: the appliedMapping read
 * in organization-drafts and the submittedSnapshot read in the organization
 * adapter. Each call renders exactly the replaced literal; only the action
 * literal and the ordering tail vary. */
export const snapshotJsonByAction = (action: string, orderTail: string) =>
  `SELECT snapshot_json FROM organization_node_change_events WHERE change_id=? AND action='${action}' ORDER BY ${orderTail} DESC LIMIT 1`;
