import 'server-only';

/** Closed recipient projection used in Collaboration's supplied notification statement. */
export function followupNotificationSource() {
  return `SELECT ?,company_id,owner_id,'customer_followup',id,version,CASE WHEN status='open' THEN 'Customer follow-up assigned: review next action and due date' ELSE 'Customer follow-up outcome recorded' END,updated_at FROM customer_followups WHERE id=?`;
}
