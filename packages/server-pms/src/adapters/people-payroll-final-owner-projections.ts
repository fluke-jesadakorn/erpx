import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function peoplePayrollFinalOwnerProjections(){return {
  "attachments_finance_facts_pms_records_r_from": "FROM (SELECT \"id\",\"property_id\",\"kind\",\"data_json\",\"version\",\"updated_at\" FROM pms_records) r",
  "attachments_finance_facts_pms_properties_p_join": "JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties) p"
} as const;}
