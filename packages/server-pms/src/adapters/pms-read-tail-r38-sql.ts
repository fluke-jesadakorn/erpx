/** Byte-identical PMS read-tail r38 source projection head shared by the
 * owner-local read-repository CTEs. Text only: each consumer keeps its own
 * suffix, binds, result type and transaction; nothing is cached or executed
 * here. Locking reads, the r17-r37 fragments and the native read registry stay
 * untouched. The two retired home count heads moved to the native hotelMetrics
 * read plan; their r56 renderer was deleted with them. */

/** Rank 3: stay source projection head shared by three read-repository CTEs. */
export const PMS_STAY_SOURCE_PROJECTION_SQL = "SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay'";
