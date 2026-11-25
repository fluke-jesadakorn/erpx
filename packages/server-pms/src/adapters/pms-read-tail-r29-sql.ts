/** Byte-identical and closed-variant PMS read-tail r29 SQL shared across owner-local sources.
 * Text only: every consumer keeps its own guards, binds, result type and transaction;
 * nothing is cached or executed here.
 * Locking reads (FOR UPDATE / advisory), r17-r28 constants and the day-report registry are untouched. */
import { pmsOwnerProjectionHeadSql } from './pms-read-tail-r53-sql';

/** Closed bare pms_postings projection with explicit owned columns. */
export const PMS_POSTINGS_PROJECTION_SQL = pmsOwnerProjectionHeadSql('postings');

/** Closed pms_properties ten-column JOIN wrapper with explicit owned columns. */
export const PMS_PROPERTIES_TEN_COL_JOIN_SQL = "JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties) p";

/** Closed booking-payment id point probes: column and status are literal unions, never caller SQL. */
export const pmsBookingPaymentIdProbeSql = (
    column: 'property_id' | 'reservation_id',
    status: 'payment_pending' | 'confirmed',
) => `SELECT id FROM pms_booking_payments WHERE ${column}=? AND status='${status}' LIMIT 1`;
