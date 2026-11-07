// Generated from owner field_outbox_quarantine.sql; scripts/docs/generate-field-outbox-predicates.ts
export const fieldOutboxQuarantine = "(e.event_type LIKE 'field.order.%' OR e.event_type LIKE 'field.agreement.%')\n";
