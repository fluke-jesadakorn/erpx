import "server-only";
import { PMS_IMPORT_BATCHES_FROM_SQL, PMS_POSTINGS_FROM_SQL } from './pms-read-tail-r19-sql';
import { PMS_POSTINGS_PROJECTION_SQL, PMS_PROPERTIES_TEN_COL_JOIN_SQL } from './pms-read-tail-r29-sql';
import { pmsFinanceSnapshotWrapperSql } from './pms-read-tail-r31-sql';
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){return {
  "hospitality_tax_documents_pms_properties_p_join": PMS_PROPERTIES_TEN_COL_JOIN_SQL,
  "hospitality_tax_documents_pms_documents_d_join": pmsFinanceSnapshotWrapperSql('documents','JOIN','d'),
  "pms_command_facts_pms_import_batches_from": PMS_IMPORT_BATCHES_FROM_SQL,
  "pms_command_facts_pms_postings_from": PMS_POSTINGS_FROM_SQL,
  "pms_finance_facts_pms_import_batches_from": PMS_IMPORT_BATCHES_FROM_SQL,
  "pms_finance_facts_pms_postings_from": PMS_POSTINGS_FROM_SQL,
  "pms_finance_facts_pms_properties_p_from": pmsFinanceSnapshotWrapperSql('properties','FROM','p'),
  "pms_finance_facts_pms_postings_pp_from": `FROM (${PMS_POSTINGS_PROJECTION_SQL}) pp`,
  "pms_finance_facts_pms_import_batches_b_join": pmsFinanceSnapshotWrapperSql('importBatches','JOIN','b'),
  "pms_finance_facts_pms_import_balances_ib_from": pmsFinanceSnapshotWrapperSql('importBalances','FROM','ib'),
  "pms_finance_facts_pms_properties_p_join": PMS_PROPERTIES_TEN_COL_JOIN_SQL,
  "pms_finance_facts_pms_records_r_from": pmsFinanceSnapshotWrapperSql('records','FROM','r'),
  "pms_finance_facts_pms_documents_d_from": pmsFinanceSnapshotWrapperSql('documents','FROM','d'),
  "pms_record_details_facts_pms_import_batches_from": PMS_IMPORT_BATCHES_FROM_SQL,
  "pms_record_details_facts_pms_postings_from": PMS_POSTINGS_FROM_SQL,
  "pms_reporting_facts_pms_import_batches_from": PMS_IMPORT_BATCHES_FROM_SQL,
  "pms_reporting_facts_pms_postings_from": PMS_POSTINGS_FROM_SQL
} as const;}
