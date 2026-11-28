/** Byte-identical PMS read-tail r20 SQL shared across owner-local projection sources.
 * Text only: every consumer keeps its own guards, filters, binds and transaction;
 * nothing is cached or executed here.
 * Locking reads (FOR UPDATE / advisory) are intentionally not part of this set. */

/** Closed pms_properties projection with explicit owned columns. */
export const PMS_PROPERTIES_PROJECTION_SQL = "SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties";

/** Closed pms_records projection with explicit owned columns. */
export const PMS_RECORDS_PROJECTION_SQL = "SELECT \"id\",\"property_id\",\"kind\",\"data_json\",\"version\",\"updated_at\" FROM pms_records";
