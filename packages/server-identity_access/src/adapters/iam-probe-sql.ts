/** Owner-local fixed authorization probes shared by the calendar,
 * collaboration and legal facts ports. One literal per probe serves every
 * call site so they cannot drift apart; the native identity role and principal
 * readers own the same authorization shape. */
export const roleAllowsSql = "SELECT 1 WHERE iam_role_allows(?,?,?)";
export const samePersonSql = "SELECT 1 WHERE iam_same_person(?,?,?)";
/** pg positional-parameter probe returning a `same` column, shared by the IAM transaction adapters. */
export const samePersonQuerySql = "SELECT iam_same_person($1,$2,$3) same";
/** pg positional-parameter probe returning an `ok` column, shared by the IAM assignment-active transaction adapters. */
export const assignmentActiveQuerySql = "SELECT iam_assignment_active($1,$2) ok";
