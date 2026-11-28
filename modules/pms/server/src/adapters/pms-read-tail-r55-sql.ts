/** Byte-identical PMS read-tail r55 owner projection head shared by the
 * pms_documents guest-account point read and the pms_stripe_payments point
 * read. Text only: the single query head token lives here; each consumer
 * keeps its own tail, binds, result type and transaction. Nothing is cached
 * or executed here, and locking reads, the native registry and the r17-r54
 * fragments stay untouched. */
export const PMS_GUEST_ACCOUNT_PROJECTION_HEAD_SQL = 'SELECT id,guest_account_id,';
