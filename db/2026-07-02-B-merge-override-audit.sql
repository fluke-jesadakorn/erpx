-- 2026-07-02-B: Merge ceo_overrides + stage_override_audit
-- into a single polymorphic approval_override_audit table.
--
-- Refactor: tier 1 step B.
-- ceo_overrides becomes kind='granted' rows.
-- stage_override_audit becomes kind='denied' rows; the broken
-- literal entity_id = 0 (the pre-existing audit bug) is mapped to NULL.
--
-- App code must be updated to use recordOverride() helper from
-- app/src/lib/approval/recordOverride.ts.

BEGIN;

-- B1. New polymorphic override audit -------------------------------------------
CREATE TABLE IF NOT EXISTS approval_override_audit (
  id              BIGSERIAL    PRIMARY KEY,
  target_type     VARCHAR(20)  NOT NULL
                    CHECK (target_type IN ('expense','pr','po')),
  target_id       INT,
  actor_id        INT REFERENCES users(id) ON DELETE SET NULL,
  kind            VARCHAR(20)  NOT NULL
                    CHECK (kind IN ('granted','denied')),
  attempted_stage VARCHAR(50),
  required_role   VARCHAR(50),
  actor_role      VARCHAR(50),
  reason          TEXT,
  created_at      TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- B2. Backfill: ceo_overrides → approval_override_audit (kind='granted') -------
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.tables
              WHERE table_schema='public' AND table_name='ceo_overrides') THEN
    INSERT INTO approval_override_audit
      (id, target_type, target_id, actor_id, kind, attempted_stage,
       required_role, actor_role, reason, created_at)
    SELECT co.id, co.target_type, co.target_id, co.actor_id,
           'granted', NULL, NULL, NULL, co.reason, co.created_at
    FROM ceo_overrides co
    ON CONFLICT (id) DO NOTHING;

    PERFORM setval(
      pg_get_serial_sequence('approval_override_audit','id'),
      GREATEST(
        (SELECT COALESCE(MAX(id),0) FROM approval_override_audit),
        (SELECT COALESCE(MAX(id),0) FROM ceo_overrides)
      )
    );
  END IF;
END$$;

-- B3. Backfill: stage_override_audit (kind='denied'; entity_id=0 → NULL) -------
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.tables
              WHERE table_schema='public' AND table_name='stage_override_audit') THEN
    INSERT INTO approval_override_audit
      (id, target_type, target_id, actor_id, kind, attempted_stage,
       required_role, actor_role, reason, created_at)
    SELECT soa.id, soa.entity_type, NULLIF(soa.entity_id, 0), soa.actor_id,
           'denied', soa.attempted_stage, soa.required_role, soa.actor_role,
           soa.reason, soa.created_at
    FROM stage_override_audit soa
    ON CONFLICT (id) DO NOTHING;

    PERFORM setval(
      pg_get_serial_sequence('approval_override_audit','id'),
      GREATEST(
        (SELECT COALESCE(MAX(id),0) FROM approval_override_audit),
        (SELECT COALESCE(MAX(id),0) FROM stage_override_audit)
      )
    );
  END IF;
END$$;

-- B4. Drop legacy tables -------------------------------------------------------
DROP TABLE IF EXISTS ceo_overrides        CASCADE;
DROP TABLE IF EXISTS stage_override_audit  CASCADE;

-- B5. Indexes ------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_aov_target
  ON approval_override_audit (target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_aov_actor
  ON approval_override_audit (actor_id);
CREATE INDEX IF NOT EXISTS idx_aov_kind_stage
  ON approval_override_audit (kind, attempted_stage);

COMMIT;

-- Verify -----------------------------------------------------------------------
SELECT kind, COUNT(*) AS n FROM approval_override_audit GROUP BY kind ORDER BY kind;