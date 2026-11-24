import 'server-only';
import {activeNodeKindExists} from './organization-column-alias-families-r34';
import {activeNodeByIdCompanyKind} from './organization-column-alias-families-r36';
/** Shared owner read-tail SQL literals. Text only: callers keep their own scope/authority filters. */
export const STAFF_LEVELS_L_JOIN =
  'JOIN (SELECT "id", "company_id", "code", "name", "rank", "active", "version", "created_at", "updated_at", "level_number" FROM staff_levels) l';
/** r34: delegates to the kind-parameterized family. Byte-identical to the
 * previous literal; the three assertion call sites are unchanged. */
export const NODE_BRANCH_ACTIVE_EXISTS = activeNodeKindExists('branch');
/** r36: delegates to the column/kind-parameterized active-node family.
 * Byte-identical to the previous literal; callers keep their own binds. */
export const NODE_BRANCH_ACTIVE = activeNodeByIdCompanyKind('1', 'branch');
/** Company+department absence guard, parameterized by table. Both relocation
 * guards render the exact same SQL; only the relation name varies. Text only:
 * callers keep their own binds and supplied statement. */
export const departmentSnapshotMissing = (table: string) => `NOT EXISTS(SELECT 1 FROM ${table} WHERE company_id=? AND department_id=?)`;
/** Table-parameterized atomic owner version guard. Both attachment owner facts
 * render byte-identical SQL; only the relation (and optional column/extra)
 * varies. Text only: callers keep their own binds and supplied statement. */
export const ownerVersionGuardSource = (table: string, column = 'version', extra?: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND ${column}=?${extra ? ` AND ${extra}` : ''})`;
