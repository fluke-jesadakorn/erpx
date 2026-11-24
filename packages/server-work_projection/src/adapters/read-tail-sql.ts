/** Shared calendar read-tail SQL fragments. One definition serves every fact unit. */
export const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
/** Point read of a row version by id and company. One definition per owned table. */
export const versionById = (table: 'calendar_appointments' | 'calendar_announcements') =>
    `SELECT version FROM ${table} WHERE id=? AND company_id=?`;
/** Inert placeholder returned to native owner-write callers so batches keep their shape. One definition serves every caller. */
export const noopSql = "SELECT 1";
