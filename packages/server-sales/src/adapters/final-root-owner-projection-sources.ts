import { commercialOwnerProjectionSources } from "./commercial-owner-projection-sources";
import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller.
 *
 * The notification and follow-up projections are Sales-owned fixed SQL also
 * served by the commercial-owner source. Both public surfaces reuse one
 * statement so the two callers cannot drift into separate query text. */
export const finalRootOwnerProjectionSources = {
  commerce_customer_notifications: commercialOwnerProjectionSources.commerce_customer_notifications,
  customer_contact_note_history: "SELECT \"id\",\"note_id\",\"version\",\"action\",\"body\",\"reason\",\"actor_id\",\"created_at\" FROM customer_contact_note_history",
  customer_contact_notes: "SELECT \"id\",\"company_id\",\"customer_id\",\"order_id\",\"channel\",\"occurred_at\",\"body\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM customer_contact_notes",
  customer_followup_events: "SELECT \"id\",\"followup_id\",\"version\",\"action\",\"actor_id\",\"owner_id\",\"due_date\",\"reason\",\"created_at\" FROM customer_followup_events",
  customer_followups: commercialOwnerProjectionSources.customer_followups,
} as const;
