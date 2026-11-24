import "server-only";
import { PMS_IMPORT_BATCHES_FROM_SQL, PMS_POSTINGS_FROM_SQL } from './pms-read-tail-r19-sql';
import { PMS_POSTINGS_PROJECTION_SQL, PMS_PROPERTIES_TEN_COL_JOIN_SQL } from './pms-read-tail-r29-sql';
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){return {
  "hospitality_tax_documents_pms_properties_p_join": PMS_PROPERTIES_TEN_COL_JOIN_SQL,
  "hospitality_tax_documents_pms_documents_d_join": "JOIN (SELECT \"id\",\"property_id\",\"guest_account_id\",\"kind\",\"finance_id\",\"snapshot_json\",\"created_at\" FROM pms_documents) d",
  "pms_command_facts_pms_import_batches_from": PMS_IMPORT_BATCHES_FROM_SQL,
  "pms_command_facts_pms_postings_from": PMS_POSTINGS_FROM_SQL,
  "pms_finance_facts_pms_import_batches_from": PMS_IMPORT_BATCHES_FROM_SQL,
  "pms_finance_facts_pms_postings_from": PMS_POSTINGS_FROM_SQL,
  "pms_finance_facts_pms_properties_p_from": "FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties) p",
  "pms_finance_facts_pms_postings_pp_from": `FROM (${PMS_POSTINGS_PROJECTION_SQL}) pp`,
  "pms_finance_facts_pms_import_batches_b_join": "JOIN (SELECT \"id\",\"company_id\",\"property_id\",\"cutover_date\",\"property_version\",\"definition_json\",\"report_json\",\"state\",\"created_by\",\"approved_by\",\"opening_journal_id\",\"version\",\"created_at\",\"updated_at\" FROM pms_import_batches) b",
  "pms_finance_facts_pms_import_balances_ib_from": "FROM (SELECT \"batch_id\",\"source_key\",\"property_id\",\"guest_account_id\",\"entry_id\",\"pocket\",\"amount_minor\",\"finance_payment_id\",\"finance_invoice_id\" FROM pms_import_balances) ib",
  "pms_finance_facts_pms_properties_p_join": PMS_PROPERTIES_TEN_COL_JOIN_SQL,
  "pms_finance_facts_pms_records_r_from": "FROM (SELECT \"id\",\"property_id\",\"kind\",\"data_json\",\"version\",\"updated_at\" FROM pms_records) r",
  "pms_finance_facts_pms_documents_d_from": "FROM (SELECT \"id\",\"property_id\",\"guest_account_id\",\"kind\",\"finance_id\",\"snapshot_json\",\"created_at\" FROM pms_documents) d",
  "pms_record_details_facts_pms_import_batches_from": PMS_IMPORT_BATCHES_FROM_SQL,
  "pms_record_details_facts_pms_postings_from": PMS_POSTINGS_FROM_SQL,
  "pms_reporting_facts_pms_import_batches_from": PMS_IMPORT_BATCHES_FROM_SQL,
  "pms_reporting_facts_pms_postings_from": PMS_POSTINGS_FROM_SQL
} as const;}
