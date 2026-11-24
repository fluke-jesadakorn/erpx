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
/** Invited-attendee scope arm of the candidate query. One definition serves every scope arm. */
export const attendeeScopeExistsSql = `EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?)`;
/** Receipt point read by id. The column set and receipt table are the only variation. */
export const receiptByIdSql = (columns: string, table: string) =>
    `SELECT ${columns} FROM ${table} WHERE id=?`;
/** staff_daily_plans composite-key read. The selected column and trailing predicate are the only variation. */
export const dailyPlanSql = (columns: string, extra = '') =>
    `SELECT ${columns} FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=?${extra}`;
/** Calendar source image snapshot by id and company. The projection expression and source table are the only variation. */
export const calendarImageSql = (expression: string, table: 'calendar_announcements' | 'calendar_appointments') =>
    `SELECT ${expression} image FROM ${table} row WHERE id=? AND company_id=?`;
/** Appointment/attendee/series joined range read. The projection columns and predicates are the only variation between the availability and reminder fact units; the join shape and series column set are shared. */
export const appointmentAttendeeSeriesSql = (columns: string, where: string) =>
    `SELECT ${columns} FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id LEFT JOIN calendar_series s ON s.appointment_id=a.id WHERE ${where}`;
/** Nested child-image projection keyed by appointment and company. Only the projection expression, source table and aliases vary across the calendar image and reminder configuration builders. */
export const appointmentChildImageSql = (expression: string, table: string, alias: string, row: string) =>
    `(SELECT ${expression} FROM ${table} ${alias} WHERE ${alias}.appointment_id=${row}.id AND ${alias}.company_id=${row}.company_id)`;
/** Command receipt existence by command id, actor, operation and company. The alias and the extra predicates before/after the company check are the only variation between the calendar purchase and reminder binding callers. */
export const commandReceiptSql = (beforeCompany = '', afterCompany = '', alias = '') => {
    const p = alias ? `${alias}.` : '';
    return `SELECT 1 FROM command_receipts${alias ? ` ${alias}` : ''} WHERE ${p}command_id=? AND ${p}actor_id=? AND ${p}operation=?${beforeCompany} AND ${p}authority_json::jsonb->>'companyId'=?${afterCompany}`;
};
