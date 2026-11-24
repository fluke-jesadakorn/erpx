/** Shared calendar read-tail SQL fragments. One definition serves every fact unit. */
export const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
/** Point read of a row version by id and company. One definition per owned table. */
export const versionById = (table: 'calendar_appointments' | 'calendar_announcements') =>
    `SELECT version FROM ${table} WHERE id=? AND company_id=?`;
/** Inert placeholder returned to native owner-write callers so batches keep their shape. One definition serves every caller. */
export const noopSql = "SELECT 1";
/** Calendar appointment existence probe. Columns and an optional predicate are the only variation. */
export const appointmentProbe = (columns: string, extra = '') =>
    `SELECT ${columns} FROM calendar_appointments WHERE id=? AND company_id=?${extra}`;
/** Confirms the actor is an invited attendee of the calendar appointment. One definition serves every authority check and inbox source. */
export const invitedAttendeeSql = "SELECT 1 FROM calendar_attendees WHERE appointment_id=? AND company_id=? AND user_id=?";
