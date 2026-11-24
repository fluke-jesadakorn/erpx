/** Byte-identical PMS read-tail r38 count/projection heads shared across owner-local
 * sources. Text only: every consumer keeps its own suffix, binds, result type and
 * transaction; nothing is cached or executed here. Locking reads, the r17-r37
 * fragments and the native read registry stay untouched. */

/** Rank 1: active-room count subquery head shared by three home metric columns. */
export const PMS_HOME_ACTIVE_ROOM_COUNT_SQL = "SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'";

/** Rank 2: stay-state count subquery head shared by three home metric columns. */
export const PMS_HOME_STAY_STATE_COUNT_SQL = "SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'";

/** Rank 3: stay source projection head shared by three read-repository CTEs. */
export const PMS_STAY_SOURCE_PROJECTION_SQL = "SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay'";
