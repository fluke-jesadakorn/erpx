-- Retain only an already captured reservation quote. Never infer a historic
-- currency/rate from today's property configuration or the latest FX table.
INSERT INTO money_transaction_snapshots (
 company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,
 amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,
 carried_source_type,carried_source_id,carried_amount_kind)
SELECT p.company_id,'pms_stay_price',s.id||':'||s.version,price->>'date',m.transaction_date,
 m.currency,m.currency_scale,(price->>'total')::numeric,
 round(m.usd_units*(price->>'total')::numeric/m.amount_units),
 m.fx_rate_id,m.used_latest_fallback,m.captured_at,
 m.source_type,m.source_id,m.amount_kind
FROM pms_records s JOIN pms_properties p ON p.id=s.property_id
JOIN money_transaction_snapshots m ON m.company_id=p.company_id
 AND m.source_type='pms_reservation' AND m.source_id=s.data_json::jsonb->>'reservationId'
 AND m.amount_kind='room_total' AND m.amount_units>0 AND m.original_source_type IS NULL
CROSS JOIN LATERAL jsonb_array_elements(COALESCE(s.data_json::jsonb->'prices','[]'::jsonb)) price
WHERE s.kind='stay' AND price->>'date' ~ '^\d{4}-\d{2}-\d{2}$'
 AND price->>'total' ~ '^\d+$' AND (price->>'total')::numeric>0
 AND (price->>'total')::numeric<=m.amount_units
 AND NOT EXISTS (SELECT 1 FROM money_transaction_snapshots prior
  WHERE prior.company_id=p.company_id AND prior.source_type='pms_stay_price'
   AND prior.source_id LIKE s.id||':%' AND prior.amount_kind=price->>'date')
ON CONFLICT(company_id,source_type,source_id,amount_kind) DO NOTHING;
