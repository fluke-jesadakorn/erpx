-- folio/db/2026-07-10-D-waybill-event-gl-confirmed.sql
--
-- Add 'gl-confirmed' kind to waybill_events.kind CHECK constraint.
-- A 'gl-confirmed' event is recorded when an accounting officer /
-- supervisor / manager / finance user explicitly confirms that the
-- GL post (kind='posted-to-gl') is correct. Decoupled from the
-- initial 'settled' / 'posted-to-gl' chain so an independent set
-- of eyes can sign off before the audit closes.

BEGIN;

ALTER TABLE waybill_events DROP CONSTRAINT waybill_events_kind_check;

ALTER TABLE waybill_events
  ADD CONSTRAINT waybill_events_kind_check
  CHECK (kind IN (
    'created',
    'submitted',
    'advanced',
    'rejected',
    'corrected',
    'settled',
    'posted-to-gl',
    'gl-confirmed',
    'slip-attached',
    'attached',
    'signed-off',
    'reversed',
    'authorization-overridden',
    'resubmitted',
    'superseded'
  ));

INSERT INTO perm.audit (kind, target)
VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-10-D-waybill-event-gl-confirmed',
    'kind_added', 'gl-confirmed'
  )
);

COMMIT;
