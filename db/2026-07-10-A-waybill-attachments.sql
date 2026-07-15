-- folio/db/2026-07-10-A-waybill-attachments.sql
--
-- Document attachments per Waybill pipeline stage.
-- Each pipeline step (submission -> ... -> disbursed) may have one or
-- more files attached by the acting role. Storage is in MinIO bucket
-- `folio-storage` keyed WB-YYYY-NNNNNN/<stage>/<uuid><ext>.
--
-- The kind taxonomy (pr_doc, po_doc, payment_receipt, slip, invoice,
-- wht_cert, signoff_memo, photo, other) is enforced via CHECK so the
-- audit log can be filtered deterministically.
--
-- Writes go through `lib/waybill/attachments.ts` -> `recordAttachment()`
-- which also writes a `kind='attached'` row to waybill_events so the
-- linked-list audit chain preserves chronology.

BEGIN;

CREATE TABLE waybill_attachments (
  id                bigserial    PRIMARY KEY,
  waybill_id        text         NOT NULL REFERENCES waybills(id) ON DELETE RESTRICT,
  stage_key         text         NOT NULL,
  kind              text         NOT NULL,
  storage_backend   text         NOT NULL DEFAULT 'minio',
  storage_key       text         NOT NULL,
  filename          text         NOT NULL,
  content_type      text         NOT NULL,
  byte_size         integer      NOT NULL CHECK (byte_size >= 0),
  uploaded_by       integer      NOT NULL,
  uploaded_role     text         NOT NULL,
  caption           text,
  occurred_at       timestamptz  NOT NULL DEFAULT now(),
  created_at        timestamptz  NOT NULL DEFAULT now(),

  CONSTRAINT waybill_attachments_kind_check
    CHECK (kind IN ('slip','pr_doc','po_doc','payment_receipt',
                    'signoff_memo','invoice','wht_cert','photo','memo','other')),
  CONSTRAINT waybill_attachments_storage_backend_check
    CHECK (storage_backend IN ('minio'))
);

CREATE INDEX idx_waybill_attachments_wb_created
  ON waybill_attachments(waybill_id, occurred_at DESC);

CREATE INDEX idx_waybill_attachments_wb_stage
  ON waybill_attachments(waybill_id, stage_key);

CREATE INDEX idx_waybill_attachments_wb_kind
  ON waybill_attachments(waybill_id, kind);

REVOKE UPDATE, DELETE ON waybill_attachments FROM n8n_user, contract;

INSERT INTO perm.audit (kind, target)
VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-10-A-waybill-attachments',
    'tables_added', jsonb_build_array('waybill_attachments'),
    'indexes_added', jsonb_build_array(
      'idx_waybill_attachments_wb_created',
      'idx_waybill_attachments_wb_stage',
      'idx_waybill_attachments_wb_kind'
    )
  )
);

COMMIT;
