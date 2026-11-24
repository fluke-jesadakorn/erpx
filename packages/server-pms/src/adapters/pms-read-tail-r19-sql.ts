/** Byte-identical PMS read-tail r19 SQL shared across owner-local projection sources.
 * Text only: every call site still supplies its own binds and result type;
 * nothing is cached or executed here.
 * Locking reads (FOR UPDATE / advisory) are intentionally not part of this set. */

/** Closed pms_import_batches projection fragment with explicit owned columns. */
export const PMS_IMPORT_BATCHES_FROM_SQL = "FROM (SELECT \"id\",\"company_id\",\"property_id\",\"cutover_date\",\"property_version\",\"definition_json\",\"report_json\",\"state\",\"created_by\",\"approved_by\",\"opening_journal_id\",\"version\",\"created_at\",\"updated_at\" FROM pms_import_batches) pms_import_batches";

/** Closed pms_postings projection fragment with explicit owned columns. */
export const PMS_POSTINGS_FROM_SQL = "FROM (SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings) pms_postings";
