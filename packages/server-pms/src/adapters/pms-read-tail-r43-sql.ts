/** Byte-identical PMS read-tail r43 query heads shared across owner-local sources.
 * Text only: every consumer keeps its own suffix, binds, result type and
 * transaction; nothing is cached or executed here. Variants come from closed
 * unions owned by this module, never caller SQL. Locking reads, the r17-r42
 * fragments and the native read registry stay untouched. */

/** Rank 1: night-source witness. The lineage CTE name is a closed union; the
 * witness takes no binds and its enclosing WHERE stays at each site. */
type NightSourceKind = 'entry_lineage' | 'lineage';
export const pmsNightSourceWitnessSql = (kind: NightSourceKind): string =>
  `EXISTS(SELECT 1 FROM ${kind} l WHERE l.entry_id=e.id AND l.source_key LIKE 'night:%')`;

/** Rank 2: pending booking-email head. The projection is a closed union; the
 * WHERE tail, DESC/LIMIT and the single property_id bind stay byte-identical. */
type BookingEmailProjection = 'reservationId' | 'full';
const BOOKING_EMAIL_PROJECTION: Record<BookingEmailProjection, string> = {
  reservationId: 'reservation_id',
  full: 'id,reservation_id,state,attempts,last_error',
};
const BOOKING_EMAIL_TAIL = "FROM pms_booking_emails WHERE property_id=? AND state!='sent' ORDER BY created_at DESC LIMIT 50";
export const pmsBookingEmailPendingSql = (kind: BookingEmailProjection): string =>
  `SELECT ${BOOKING_EMAIL_PROJECTION[kind]} ${BOOKING_EMAIL_TAIL}`;

/** Rank 3: recursive lineage step head. The CTE name and the optional
 * business_date column are closed variants; the NOT LIKE termination and the
 * ANY(visited) cycle guard are shared verbatim. */
type NightLineageKind = 'entry_lineage' | 'lineage';
const NIGHT_LINEAGE_COLUMNS: Record<NightLineageKind, { indent: string; columns: string }> = {
  entry_lineage: {
    indent: '        ',
    columns: "l.entry_id,l.property_id,l.business_date,l.net_raw,parent.id,\n          parent.data_json::jsonb->>'sourceKey',parent.data_json::jsonb->>'originalId',l.visited||parent.id",
  },
  lineage: {
    indent: '      ',
    columns: "l.entry_id,l.property_id,l.net_raw,parent.id,parent.data_json::jsonb->>'sourceKey',\n        parent.data_json::jsonb->>'originalId',l.visited||parent.id",
  },
};
export const pmsNightLineageStepSql = (kind: NightLineageKind): string => {
  const { indent, columns } = NIGHT_LINEAGE_COLUMNS[kind];
  return `${indent}SELECT ${columns}
${indent}FROM ${kind} l JOIN pms_records parent ON parent.id=l.original_id AND parent.property_id=l.property_id AND parent.kind='entry'
${indent}WHERE COALESCE(l.source_key,'') NOT LIKE 'night:%' AND NOT (parent.id=ANY(l.visited))`;
};
