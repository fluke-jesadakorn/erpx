-- 2026-07-02-A: Merge approval_logs + pr_approval_logs + po_approval_logs
-- into a single polymorphic approval_transitions table.
--
-- Refactor: tier 1 step A from the 2026-07-02 refactor plan.
-- Backwards-incompatible: drops 3 legacy tables. App code must be updated
-- in the same deploy to use recordTransition() helper from
-- app/src/lib/approval/recordTransition.ts.
--
-- Idempotent: safe to re-run only AFTER dropping the legacy tables.
-- Each block is guarded by IF EXISTS / IF NOT EXISTS where possible.

BEGIN;

-- A1. New polymorphic table -----------------------------------------------------
CREATE TABLE IF NOT EXISTS approval_transitions (
  id              BIGSERIAL    PRIMARY KEY,
  target_type     VARCHAR(20)  NOT NULL
                    CHECK (target_type IN ('expense','pr','po')),
  target_id       INT          NOT NULL,
  actor_id        INT REFERENCES users(id) ON DELETE SET NULL,
  previous_status VARCHAR(50),
  new_status      VARCHAR(50),
  comments        TEXT,
  stage           VARCHAR(50),
  chain_index     INT,
  created_at      TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- A2. Backfill: approval_logs → approval_transitions ---------------------------
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.tables
              WHERE table_schema='public' AND table_name='approval_logs') THEN
    INSERT INTO approval_transitions
      (id, target_type, target_id, actor_id, previous_status, new_status,
       comments, stage, chain_index, created_at)
    SELECT id, 'expense', expense_id, actor_id, previous_status, new_status,
           comments, stage, chain_index, created_at
    FROM approval_logs
    ON CONFLICT (id) DO NOTHING;

    PERFORM setval(
      pg_get_serial_sequence('approval_transitions','id'),
      GREATEST(
        (SELECT COALESCE(MAX(id),0) FROM approval_transitions),
        (SELECT COALESCE(MAX(id),0) FROM approval_logs)
      )
    );
  END IF;
END$$;

-- A3. Backfill: pr_approval_logs (chain_index was NULL — kept NULL) ------------
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.tables
              WHERE table_schema='public' AND table_name='pr_approval_logs') THEN
    INSERT INTO approval_transitions
      (id, target_type, target_id, actor_id, previous_status, new_status,
       comments, stage, chain_index, created_at)
    SELECT id, 'pr', pr_id, actor_id, previous_status, new_status,
           comments, stage, NULL, created_at
    FROM pr_approval_logs
    ON CONFLICT (id) DO NOTHING;

    PERFORM setval(
      pg_get_serial_sequence('approval_transitions','id'),
      GREATEST(
        (SELECT COALESCE(MAX(id),0) FROM approval_transitions),
        (SELECT COALESCE(MAX(id),0) FROM pr_approval_logs)
      )
    );
  END IF;
END$$;

-- A4. Backfill: po_approval_logs -----------------------------------------------
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.tables
              WHERE table_schema='public' AND table_name='po_approval_logs') THEN
    INSERT INTO approval_transitions
      (id, target_type, target_id, actor_id, previous_status, new_status,
       comments, stage, chain_index, created_at)
    SELECT id, 'po', po_id, actor_id, previous_status, new_status,
           comments, stage, chain_index, created_at
    FROM po_approval_logs
    ON CONFLICT (id) DO NOTHING;

    PERFORM setval(
      pg_get_serial_sequence('approval_transitions','id'),
      GREATEST(
        (SELECT COALESCE(MAX(id),0) FROM approval_transitions),
        (SELECT COALESCE(MAX(id),0) FROM po_approval_logs)
      )
    );
  END IF;
END$$;

-- A5. Drop legacy tables -------------------------------------------------------
DROP TABLE IF EXISTS approval_logs    CASCADE;
DROP TABLE IF EXISTS pr_approval_logs CASCADE;
DROP TABLE IF EXISTS po_approval_logs CASCADE;

-- A6. Indexes ------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_atx_target
  ON approval_transitions (target_type, target_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_atx_actor
  ON approval_transitions (actor_id);
CREATE INDEX IF NOT EXISTS idx_atx_stage
  ON approval_transitions (stage) WHERE stage IS NOT NULL;

COMMIT;

-- Verify row counts preserved --------------------------------------------------
SELECT 'approval_transitions' AS src, target_type, COUNT(*) AS n
  FROM approval_transitions GROUP BY target_type ORDER BY target_type;