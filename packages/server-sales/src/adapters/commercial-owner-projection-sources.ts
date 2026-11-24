import { tableRead } from "./followup-source-sql";
import "server-only";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  commerce_customer_notification_settings: tableRead('"company_id","adapter","version","updated_by","updated_at"', "commerce_customer_notification_settings"),
  commerce_customer_notifications: tableRead('"id","company_id","branch_id","order_id","fulfillment_id","party_id","party_version","recipient_email","subject","body_text","adapter","status","version","attempts","next_attempt_at","lease_until","provider_message_id","sent_at","last_error","created_by","created_at","updated_at"', "commerce_customer_notifications"),
  customer_followups: tableRead('"id","company_id","order_id","owner_id","due_date","next_action","status","outcome","version","created_by","created_at","updated_at"', "customer_followups"),
} as const;
