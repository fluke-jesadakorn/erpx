/** Byte-identical Project member guard shared by the Project purchase source
 * read and the Project attachment policy read. The project/member aliases are
 * the only degrees of freedom; the relation, equality and bind placeholder stay
 * exactly as in both former inline copies. No SQL comes from a caller. */
export const projectMemberExistsSql = (project = 'p', member = 'm') =>
    `EXISTS(SELECT 1 FROM staff_project_members ${member} WHERE ${member}.project_id=${project}.id AND ${member}.user_id=?)`;
