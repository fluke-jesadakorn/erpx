/** Byte-identical and parameterized PMS read-tail r33 SQL shared across owner-local sources.
 * Text only: every consumer keeps its own binds, result type and transaction;
 * nothing is cached or executed here. The source is a closed union owned by this
 * module, never caller SQL.
 * Locking reads, r17-r32 fragments and the native POS-stripe read registry are untouched. */

/** Closed POS property-config point read. The source selects a fixed table and
 * alias from a closed union; the projection, predicate and bind are owned text. */
export const pmsPosPropertyConfigSql = (source: 'booking' | 'stripe'): string => {
  const { table, alias } = source === 'booking'
    ? { table: 'pms_booking_payments', alias: 'b' }
    : { table: 'pms_stripe_payments', alias: 'm' };
  return `SELECT p.config_json FROM pms_properties p JOIN ${table} ${alias} ON ${alias}.property_id=p.id WHERE ${alias}.company_id=? ORDER BY ${alias}.created_at LIMIT 1`;
};
