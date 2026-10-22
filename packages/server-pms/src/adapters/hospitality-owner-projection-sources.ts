import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  pms_postings: "SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings",
  pms_properties: "SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties",
  pms_records: "SELECT \"id\",\"property_id\",\"kind\",\"data_json\",\"version\",\"updated_at\" FROM pms_records",
  pms_sources: "SELECT \"property_id\",\"source_key\",\"record_id\" FROM pms_sources",
} as const;
