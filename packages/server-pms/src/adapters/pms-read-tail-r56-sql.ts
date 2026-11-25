/** Byte-identical PMS read-tail r56 count head shared by the pms_home
 * active-room and stay-state count projections. Text only: the single query
 * head token lives here; each consumer keeps its own alias, predicate, binds,
 * result type and transaction. Nothing is cached or executed here, and
 * locking reads, the native registry and the r17-r55 fragments stay untouched. */
export const PMS_RECORDS_COUNT_HEAD_SQL = 'SELECT count(*) FROM pms_records ';
