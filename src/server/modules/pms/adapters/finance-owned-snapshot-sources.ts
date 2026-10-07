import "server-only";
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){return {
  "hospitality_tax_documents_pms_properties_p_join": "JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties) p",
  "hospitality_tax_documents_pms_documents_d_join": "JOIN (SELECT \"id\",\"property_id\",\"folio_id\",\"kind\",\"finance_id\",\"snapshot_json\",\"created_at\" FROM pms_documents) d",
  "pms_command_facts_pms_import_batches_from": "FROM (SELECT \"id\",\"company_id\",\"property_id\",\"cutover_date\",\"property_version\",\"definition_json\",\"report_json\",\"state\",\"created_by\",\"approved_by\",\"opening_journal_id\",\"version\",\"created_at\",\"updated_at\" FROM pms_import_batches) pms_import_batches",
  "pms_command_facts_pms_postings_from": "FROM (SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings) pms_postings",
  "pms_finance_facts_pms_import_batches_from": "FROM (SELECT \"id\",\"company_id\",\"property_id\",\"cutover_date\",\"property_version\",\"definition_json\",\"report_json\",\"state\",\"created_by\",\"approved_by\",\"opening_journal_id\",\"version\",\"created_at\",\"updated_at\" FROM pms_import_batches) pms_import_batches",
  "pms_finance_facts_pms_postings_from": "FROM (SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings) pms_postings",
  "pms_finance_facts_pms_properties_p_from": "FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties) p",
  "pms_finance_facts_pms_postings_pp_from": "FROM (SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings) pp",
  "pms_finance_facts_pms_import_batches_b_join": "JOIN (SELECT \"id\",\"company_id\",\"property_id\",\"cutover_date\",\"property_version\",\"definition_json\",\"report_json\",\"state\",\"created_by\",\"approved_by\",\"opening_journal_id\",\"version\",\"created_at\",\"updated_at\" FROM pms_import_batches) b",
  "pms_finance_facts_pms_import_balances_ib_from": "FROM (SELECT \"batch_id\",\"source_key\",\"property_id\",\"folio_id\",\"entry_id\",\"pocket\",\"amount_minor\",\"finance_payment_id\",\"finance_invoice_id\" FROM pms_import_balances) ib",
  "pms_finance_facts_pms_properties_p_join": "JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties) p",
  "pms_finance_facts_pms_records_r_from": "FROM (SELECT \"id\",\"property_id\",\"kind\",\"data_json\",\"version\",\"updated_at\" FROM pms_records) r",
  "pms_finance_facts_pms_documents_d_from": "FROM (SELECT \"id\",\"property_id\",\"folio_id\",\"kind\",\"finance_id\",\"snapshot_json\",\"created_at\" FROM pms_documents) d",
  "pms_record_details_facts_pms_import_batches_from": "FROM (SELECT \"id\",\"company_id\",\"property_id\",\"cutover_date\",\"property_version\",\"definition_json\",\"report_json\",\"state\",\"created_by\",\"approved_by\",\"opening_journal_id\",\"version\",\"created_at\",\"updated_at\" FROM pms_import_batches) pms_import_batches",
  "pms_record_details_facts_pms_postings_from": "FROM (SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings) pms_postings",
  "pms_reporting_facts_pms_import_batches_from": "FROM (SELECT \"id\",\"company_id\",\"property_id\",\"cutover_date\",\"property_version\",\"definition_json\",\"report_json\",\"state\",\"created_by\",\"approved_by\",\"opening_journal_id\",\"version\",\"created_at\",\"updated_at\" FROM pms_import_batches) pms_import_batches",
  "pms_reporting_facts_pms_postings_from": "FROM (SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings) pms_postings"
} as const;}
