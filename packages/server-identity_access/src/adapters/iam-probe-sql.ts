/** Owner-local fixed authorization probes shared by the calendar,
 * collaboration and legal facts ports. One literal per probe serves every
 * call site so they cannot drift apart; the native identity role and principal
 * readers own the same authorization shape. */
export const roleAllowsSql = "SELECT 1 WHERE iam_role_allows(?,?,?)";
export const samePersonSql = "SELECT 1 WHERE iam_same_person(?,?,?)";
/** Parameterized same-person column probe shared by the pg IAM callers (positional
 * binds) and the People evaluation facts port (qmark binds). Only the three fixed
 * placeholder tokens from those owner callers are accepted; the projection and
 * function are fixed here. No lock or control-plane read is changed. */
export const samePersonColumnSql = (a: string, b: string, c: string) =>
  `SELECT iam_same_person(${a},${b},${c}) same`;
/** pg positional-parameter probe returning a `same` column, shared by the IAM transaction adapters. */
export const samePersonQuerySql = samePersonColumnSql("$1", "$2", "$3");
/** pg positional-parameter probe returning an `ok` column, shared by the IAM assignment-active transaction adapters. */
export const assignmentActiveQuerySql = "SELECT iam_assignment_active($1,$2) ok";
/** pg positional-parameter probe returning the locked iam_members row, shared by the migration and transaction adapters. */
export const memberForShareSql = "SELECT * FROM iam_members WHERE company_id=$1 AND user_id=$2 FOR SHARE";
/** pg positional-parameter probe for a protected emergency/directory migration identity, shared by the prepare and review migration adapters. */
export const protectedMigrationIdentitySql = "SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND (login_policy IN ('emergency','directory') OR status!='active')";
/** Byte-identical active-recipient assignment read shared by the calendar and
 * collaboration facts ports. `gap` preserves each caller's original whitespace
 * between the bind list and the active predicates, so every rendered statement
 * stays byte-identical to its pre-dedupe literal. No table/alias/SQL is accepted
 * from a caller: only the already-authorized id list and its original gap. */
export const activeRecipientsSql = (ids: string[], gap: string) =>
  `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IN (${ids.map(() => "?").join(",")})${gap}AND revoked_at IS NULL AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)`;
/** Byte-identical user id/name read shared by the calendar appointments facts
 * port and the executive main-evidence source. Only the already-authorized id
 * list is accepted from a caller; the projection SQL is fixed here. */
export const idNameByUserIdsSql = (ids: string[]) =>
  `SELECT id,name FROM "user" WHERE id IN (${ids.map(() => "?").join(",")})`;
/** Byte-identical verified-email read shared by the private session preferences
 * and private session read adapters. Each caller owns its transaction; this
 * probe takes no lock and touches no lock statement. */
export const verifiedUserEmailSql = 'SELECT email,"emailVerified" verified FROM "user" WHERE id=?';
/** Byte-identical account id/name/email read shared by the IAM service reads
 * port and the member directory adapter. Only the already-authorized id list
 * is accepted from a caller; the projection SQL is fixed here. */
export const idNameEmailByUserIdsSql = (ids: string[]) =>
  `SELECT id,name,email FROM "user" WHERE id IN (${ids.map(() => "?").join(",")})`;
/** Parameterized published-role projection shared by the iam role store
 * (qmark binds) and the native grant transaction (pg positional binds). Only
 * the two fixed placeholder tokens from those owner callers are accepted; the
 * projection SQL is fixed here. No lock or control-plane read is changed. */
export const publishedRoleSql = (companyParam: string, roleParam: string) =>
  `SELECT r.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',${companyParam}) AND r.id=${roleParam} AND r.status='active'`;
/** Byte-identical active-grant probe wrapper shared by the payroll cancellation
 * and recovery grant adapters. Each caller owns its already-authorized grant
 * predicate; only the fixed single-row existence wrapper lives here so the two
 * callers cannot drift apart. No lock or control-plane read is changed. */
export const grantProbeSql = (grant: string) => `SELECT 1 WHERE ${grant}`;
/** Parameterized revoked-session probe shared by the two qmark authorization
 * reads (legal AI actor facts and record source authority) and the pg IAM
 * transaction caller. The qmark callers wrap this probe in NOT EXISTS; the pg
 * caller runs it as a standalone positional-parameter statement. Only the
 * three fixed placeholder tokens from those owner callers are accepted; the
 * probed table, projection and predicates are fixed here. No lock, write or
 * control-plane read is changed. */
export const revokedSessionProbeSql = (sessionParam: string, companyParam: string, userParam: string) =>
  `SELECT 1 FROM iam_revoked_sessions WHERE session_id=${sessionParam} AND company_id=${companyParam} AND user_id=${userParam}`;
/** Byte-identical session-by-user read shared by the IAM API session list and
 * the IAM service reads port. `tail` preserves each caller's original ORDER BY
 * suffix so every rendered statement stays byte-identical to its pre-dedupe
 * literal. No caller-supplied SQL is accepted beyond that fixed suffix. */
export const sessionByUserSql = (tail: string) =>
  `SELECT id,"createdAt","expiresAt","userAgent" FROM session WHERE "userId"=?${tail}`;
/** Byte-identical session existence probe shared by the IAM API revoke guard
 * and the collaboration push-session adapter. `tail` preserves each caller's
 * original expiresAt predicate; no lock or control-plane read is changed. */
export const sessionExistsSql = (tail: string) =>
  `SELECT 1 FROM session WHERE id=? AND "userId"=?${tail}`;
/** Byte-identical assignment grant probe shared by the two project current-authority
 * reads. They differ only by an optional published-role predicate; the caller
 * supplies that fixed tail, never user input. The table, projection, binds and
 * active/principal predicates are fixed here so they cannot drift. No lock,
 * session or earlier-round helper is changed. */
export const projectAuthorityProbeSql = (roleTail: string) =>
  `SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)${roleTail}`;
