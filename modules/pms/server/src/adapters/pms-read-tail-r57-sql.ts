/** Byte-identical PMS read-tail r57 reservation-id arm head shared by the
 * chosen/stays relation arms. Text only: the single query head token lives
 * here; each consumer keeps its own source tail, binds, result type and
 * transaction. Nothing is cached or executed here, and locking reads, the
 * native registry and the r17-r56 fragments stay untouched. */
export const PMS_RESERVATION_ID_ARM_HEAD_SQL = "SELECT data->>'reservationId' ";
