-- folio/db/2026-07-14-realign-stages-no-ddl.sql
--
-- Realign in-flight waybills currently parked at the legacy stages
-- (final_authorization, cfo_authorization, ceo_authorization, po_cfo)
-- back to accounting_authorization so they resume the new approval
-- chain (manager→supervisor fallback → accounting verification /
-- supervision / authorization → awaiting_disbursement → disbursed).
--
-- Each realigned waybill gets one stage-realigned audit row so the
-- move is visible in the linked-list trail.
--
-- Idempotent: re-running is a no-op (the UPDATE's WHERE clause
-- filters out rows already realigned, and the INSERT's NOT EXISTS
-- blocks duplicate stage-realigned rows for the same waybill).

BEGIN;

-- 1. Extend waybill_events.kind CHECK to allow 'stage-realigned'.

ALTER TABLE waybill_events DROP CONSTRAINT IF EXISTS waybill_events_kind_check;

ALTER TABLE waybill_events
  ADD CONSTRAINT waybill_events_kind_check
  CHECK (kind = ANY (ARRAY[
    'created','submitted','advanced','rejected','corrected',
    'settled','posted-to-gl','gl-confirmed','slip-attached','attached',
    'signed-off','reversed','authorization-overridden','resubmitted','superseded',
    'posted-to-gl-accrual','gl-confirmed-accrual',
    'posted-to-gl-settlement','gl-confirmed-settlement',
    'created-draft-gl-accrual','created-draft-gl-settlement',
    'stage-realigned'
  ]));

-- 2. Realign open waybills parked at the legacy stages.

UPDATE waybills
   SET current_stage = 'accounting_authorization',
       updated_at    = now()
 WHERE current_stage IN ('final_authorization','cfo_authorization',
                         'ceo_authorization','po_cfo')
   AND status         = 'open';

-- 3. Audit: one stage-realigned event per realigned waybill.

INSERT INTO waybill_events
  (waybill_id, sequence, previous_event_id, kind, stage_from, stage_to,
   actor_id, actor_role, actor_signature, payload)
SELECT w.id,
       COALESCE((SELECT MAX(sequence) FROM waybill_events WHERE waybill_id = w.id), 0) + 1,
       (SELECT id FROM waybill_events WHERE waybill_id = w.id
         ORDER BY sequence DESC LIMIT 1),
       'stage-realigned',
       'accounting_authorization',
       'accounting_authorization',
       NULL, 'system', '\x'::bytea,
       jsonb_build_object('reason', 'realign-legacy-stage',
                          'realigned_from', ARRAY['final_authorization','cfo_authorization','ceo_authorization','po_cfo'])
  FROM waybills w
 WHERE w.current_stage = 'accounting_authorization'
   AND w.status         = 'open'
   AND NOT EXISTS (
     SELECT 1 FROM waybill_events we
      WHERE we.waybill_id = w.id AND we.kind = 'stage-realigned'
   );

-- 4. Schema migration audit row.

INSERT INTO perm.audit (kind, target)
VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-14-realign-stages',
    'description', 'Realign in-flight waybills at legacy stages to accounting_authorization + extend kind CHECK with stage-realigned'
  )
);

COMMIT;
