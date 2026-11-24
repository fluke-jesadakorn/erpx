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
/** Work-cache revision read. The full snapshot and a filtered watermark subset share one definition. */
export const workCacheRevisionsSql = (sourceTables?: string[]) => {
    const revision = sourceTables?.length ? "revision::text revision" : "revision";
    const filter = sourceTables?.length ? ` WHERE table_name IN (${sourceTables.map(() => "?").join(",")})` : "";
    return `SELECT table_name,${revision} FROM work_cache_revisions${filter} ORDER BY table_name`;
};
/** Conflicting scheduled-appointment probe. Join source, extra predicate and scoped column vary; the shape is shared. */
export const overlapProbe = (join: 'calendar_appointment_participants' | 'calendar_attendees', column: string, extra = '') =>
    `SELECT 1 FROM calendar_appointments a JOIN ${join} p ON p.appointment_id=a.id WHERE a.company_id=? AND a.status='scheduled' AND a.id!=? AND a.starts_at<? AND a.ends_at>?${extra} AND ${inList(column)} LIMIT 1`;
/** Point read of a single occurrence exception. Only the selected columns vary between fact units. */
export const occurrenceExceptionPointSql = (columns: string) =>
    `SELECT ${columns} FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?`;
/** Reminder source point read over the appointment/attendee join. Only the selected columns vary. */
export const reminderAttendeePointSql = (columns: string) =>
    `SELECT ${columns} FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND t.user_id=?`;
