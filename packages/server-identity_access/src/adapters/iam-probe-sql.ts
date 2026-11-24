/** Owner-local fixed authorization probes shared by the calendar,
 * collaboration and legal facts ports. One literal per probe serves every
 * call site so they cannot drift apart; the native identity role and principal
 * readers own the same authorization shape. */
export const roleAllowsSql = "SELECT 1 WHERE iam_role_allows(?,?,?)";
export const samePersonSql = "SELECT 1 WHERE iam_same_person(?,?,?)";
