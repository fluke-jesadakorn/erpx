import 'server-only';
/** Existing independently managed customer identity and consent only. This
 * reference port grants no Finance collection or payment authority. */
export const salesNotificationCustomerSources="SELECT id,company_id,kind,version,status,notification_email,notification_consent FROM finance_parties WHERE kind='customer'";
