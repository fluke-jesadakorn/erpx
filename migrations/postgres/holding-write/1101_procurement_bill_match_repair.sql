-- Repair matches stranded by native Finance posts/voids before the
-- Procurement match lifecycle port. Idempotent per match/version event.
WITH now AS (
  SELECT (extract(epoch FROM clock_timestamp()) * 1000)::bigint AS at
), returned AS (
  UPDATE procurement_bill_matches m
  SET status = 'returned',
      invoice_id = NULL,
      version = m.version + 1,
      updated_at = (SELECT at FROM now)
  WHERE m.status IN ('invoiced', 'posted')
    AND (
      m.invoice_id IS NULL
      OR EXISTS (
        SELECT 1 FROM finance_invoices i
        WHERE i.id = m.invoice_id AND i.status = 'voided'
      )
    )
  RETURNING m.id, m.version
)
INSERT INTO procurement_bill_match_events(id, match_id, version, action, actor_id, reason, created_at)
SELECT 'bill-event-repair-returned-' || id, id, version, 'returned', 'system-repair',
       'Matched payable repair: voided invoice returned to Procurement',
       (SELECT at FROM now)
FROM returned
ON CONFLICT (match_id, version) DO NOTHING;

UPDATE procurement_bill_match_receipts br
SET active = FALSE
WHERE br.active = TRUE
  AND EXISTS (
    SELECT 1 FROM procurement_bill_matches m
    WHERE m.id = br.match_id AND m.status = 'returned' AND m.invoice_id IS NULL
  );

WITH now AS (
  SELECT (extract(epoch FROM clock_timestamp()) * 1000)::bigint AS at
), posted AS (
  UPDATE procurement_bill_matches m
  SET status = 'posted',
      version = m.version + 1,
      updated_at = (SELECT at FROM now)
  WHERE m.status = 'invoiced'
    AND m.invoice_id IS NOT NULL
    AND EXISTS (
      SELECT 1 FROM finance_invoices i
      WHERE i.id = m.invoice_id AND i.status = 'posted'
    )
  RETURNING m.id, m.version
)
INSERT INTO procurement_bill_match_events(id, match_id, version, action, actor_id, reason, created_at)
SELECT 'bill-event-repair-posted-' || id, id, version, 'posted', 'system-repair',
       'Matched payable repair: posted invoice advanced in Procurement',
       (SELECT at FROM now)
FROM posted
ON CONFLICT (match_id, version) DO NOTHING;
