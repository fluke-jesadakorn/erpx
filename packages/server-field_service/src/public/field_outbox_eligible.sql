(
 (e.aggregate_type='field_order' AND e.event_type IN ('field.order.request','field.order.create','field.order.schedule','field.order.dispatch','field.order.start','field.order.exception','field.order.resume','field.order.complete','field.order.cancel','field.order.release-billing','field.order.reserve-part','field.order.issue-part')
  AND EXISTS(SELECT 1 FROM field_service_orders s WHERE s.id=e.aggregate_id AND s.company_id=e.payload_json::jsonb->>'companyId' AND s.version>=e.aggregate_version))
 OR (e.aggregate_type='field_agreement' AND e.event_type IN ('field.agreement.create','field.agreement.activate','field.agreement.reject','field.agreement.retire')
  AND EXISTS(SELECT 1 FROM field_service_agreements s WHERE s.id=e.aggregate_id AND s.company_id=e.payload_json::jsonb->>'companyId' AND s.version>=e.aggregate_version))
)
