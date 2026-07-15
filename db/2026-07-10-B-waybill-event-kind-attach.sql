-- folio/db/2026-07-10-B-waybill-event-kind-attach.sql
--
-- Add 'attached' kind to waybill_events.kind CHECK constraint.
-- The new attachment flow writes a kind='attached' event whenever an
-- upload completes (see lib/waybill/attachments.ts recordAttachment()).

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
    'migration', '2026-07-10-B-waybill-event-kind-attach',
    'kind_added', 'attached'
  )
);

COMMIT;
