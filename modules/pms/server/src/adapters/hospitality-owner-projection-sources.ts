import "server-only";
import { PMS_PROPERTIES_PROJECTION_SQL, PMS_RECORDS_PROJECTION_SQL } from './pms-read-tail-r20-sql';
import { PMS_POSTINGS_PROJECTION_SQL } from './pms-read-tail-r29-sql';
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  pms_postings: PMS_POSTINGS_PROJECTION_SQL,
  pms_properties: PMS_PROPERTIES_PROJECTION_SQL,
  pms_records: PMS_RECORDS_PROJECTION_SQL,
  pms_sources: "SELECT \"property_id\",\"source_key\",\"record_id\" FROM pms_sources",
} as const;
