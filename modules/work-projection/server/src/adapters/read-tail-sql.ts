/** Shared calendar read-tail SQL fragments. One definition serves every fact unit. */
export const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
/** Point read of a row version by id and company. The source-row probe head is shared; only the columns differ. */
export const versionById = (table: 'calendar_appointments' | 'calendar_announcements') =>
    appointmentProbe('version', '', table);
/** Inert placeholder returned to native owner-write callers so batches keep their shape. One definition serves every caller. */
export const noopSql = "SELECT 1";
/** Calendar source-row point read by id and company. Columns, source table and an optional predicate are the only variation. */
export const appointmentProbe = (columns: string, extra = '', table: 'calendar_appointments' | 'calendar_announcements' = 'calendar_appointments') =>
    `SELECT ${columns} FROM ${table} WHERE id=? AND company_id=?${extra}`;
/** Confirms the actor is an invited attendee of the calendar appointment. One definition serves every authority check and inbox source. */
export const invitedAttendeeSql = "SELECT 1 FROM calendar_attendees WHERE appointment_id=? AND company_id=? AND user_id=?";
/** Work-cache revision read. The full snapshot and a filtered watermark subset share one definition. */
export const workCacheRevisionsSql = (sourceTables?: string[]) => {
    const revision = sourceTables?.length ? "revision::text revision" : "revision";
    const filter = sourceTables?.length ? ` WHERE table_name IN (${sourceTables.map(() => "?").join(",")})` : "";
    return `SELECT table_name,${revision} FROM work_cache_revisions${filter} ORDER BY table_name`;
};
/** Conflicting scheduled-appointment probes are owned by the native lane 29
 * reader (`native_lane_29/postgres.rs`); no TypeScript SQL remains for them. */
/** Occurrence-exception reads over one canonical head. Selected columns and the predicate tail are the only variation between the point, moved-occurrence and reminder callers. */
export const occurrenceExceptionsSql = (columns: string, tail: string) =>
    `SELECT ${columns} FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND ${tail}`;
/** Point read of a single occurrence exception. Only the selected columns vary between fact units. */
export const occurrenceExceptionPointSql = (columns: string) =>
    occurrenceExceptionsSql(columns, 'occurrence_key=?');
/** Shared appointment/attendee join head. Only the selected columns and the trailing join/predicate vary across the reminder and availability fact units. */
export const appointmentAttendeesSql = (columns: string, tail: string) =>
    `SELECT ${columns} FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id ${tail}`;
/** Reminder attendee join over appointment/attendee. Selected columns and trailing predicate are the only variation between the configuration point read and the recipients list read. */
export const reminderAttendeesSql = (columns: string, tail = '') =>
    appointmentAttendeesSql(columns, `WHERE a.id=? AND a.company_id=?${tail}`);
/** Reminder source point read over the appointment/attendee join. Only the selected columns vary. */
export const reminderAttendeePointSql = (columns: string) =>
    reminderAttendeesSql(columns, ' AND t.user_id=?');
/** Invited-attendee scope arm of the candidate query. One definition serves every scope arm. */
export const attendeeScopeExistsSql = `EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?)`;
/** Participant-scope arm of the candidate query. The joined body is the only variation between the engagement scope and the person point check. */
export const participantScopeExistsSql = (body: string) =>
    `EXISTS(SELECT 1 FROM calendar_appointment_participants ap ${body})`;
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
    appointmentAttendeesSql(columns, `LEFT JOIN calendar_series s ON s.appointment_id=a.id WHERE ${where}`);
/** Nested child-image projection keyed by appointment and company. Only the projection expression, source table and aliases vary across the calendar image and reminder configuration builders. */
export const appointmentChildImageSql = (expression: string, table: string, alias: string, row: string) =>
    `(SELECT ${expression} FROM ${table} ${alias} WHERE ${alias}.appointment_id=${row}.id AND ${alias}.company_id=${row}.company_id)`;
/** Command receipt existence by command id, actor, operation and company. The alias and the extra predicates before/after the company check are the only variation between the calendar purchase and reminder binding callers. */
export const commandReceiptSql = (beforeCompany = '', afterCompany = '', alias = '') => {
    const p = alias ? `${alias}.` : '';
    return `SELECT 1 FROM command_receipts${alias ? ` ${alias}` : ''} WHERE ${p}command_id=? AND ${p}actor_id=? AND ${p}operation=?${beforeCompany} AND ${p}authority_json::jsonb->>'companyId'=?${afterCompany}`;
};
/** Membership probe over a derived source. The source, alias and predicate are the only variation between the people-identity check and the engagement-scope check. */
export const derivedProbeSql = (source: string, alias: string, predicate: string) =>
    `SELECT 1 FROM (${source}) ${alias} WHERE ${predicate}`;
/** Bound derived scope row. The scope columns are the only variation between the workforce actor and calendar release probes. */
export const scopeRowSql = (columns: string) =>
    `SELECT ?::text ${columns}`;
