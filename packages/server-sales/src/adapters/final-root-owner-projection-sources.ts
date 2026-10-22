import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  commerce_customer_notifications: "SELECT \"id\",\"company_id\",\"branch_id\",\"order_id\",\"fulfillment_id\",\"party_id\",\"party_version\",\"recipient_email\",\"subject\",\"body_text\",\"adapter\",\"status\",\"version\",\"attempts\",\"next_attempt_at\",\"lease_until\",\"provider_message_id\",\"sent_at\",\"last_error\",\"created_by\",\"created_at\",\"updated_at\" FROM commerce_customer_notifications",
  customer_contact_note_history: "SELECT \"id\",\"note_id\",\"version\",\"action\",\"body\",\"reason\",\"actor_id\",\"created_at\" FROM customer_contact_note_history",
  customer_contact_notes: "SELECT \"id\",\"company_id\",\"customer_id\",\"order_id\",\"channel\",\"occurred_at\",\"body\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM customer_contact_notes",
  customer_followup_events: "SELECT \"id\",\"followup_id\",\"version\",\"action\",\"actor_id\",\"owner_id\",\"due_date\",\"reason\",\"created_at\" FROM customer_followup_events",
  customer_followups: "SELECT \"id\",\"company_id\",\"order_id\",\"owner_id\",\"due_date\",\"next_action\",\"status\",\"outcome\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM customer_followups",
} as const;
