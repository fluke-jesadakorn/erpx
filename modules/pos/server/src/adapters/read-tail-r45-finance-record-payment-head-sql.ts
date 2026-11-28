import 'server-only';
/** Byte-identical payment identity head shared by the retained Finance payment
 * source and its opaque source-image projection. Each caller keeps its own
 * FROM/WHERE or to_jsonb tail, so rendered SQL stays byte-identical. */
export const posFinanceRecordPaymentHeadSql = (): string => 'SELECT company_id,finance_payment_id id';
