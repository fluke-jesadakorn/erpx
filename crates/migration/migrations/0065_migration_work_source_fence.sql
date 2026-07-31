-- Every queued G13 work item must carry a content-addressed source fence.
-- Repository methods additionally require equality with the immutable run
-- contract hash; this constraint protects direct SQL and recovery writers.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'erpx_control.migration_work_items'::regclass
          AND conname = 'migration_work_items_source_fence_sha256_ck'
    ) THEN
        ALTER TABLE erpx_control.migration_work_items
            ADD CONSTRAINT migration_work_items_source_fence_sha256_ck
            CHECK (source_fence ~ '^[0-9a-fA-F]{64}$');
    END IF;
END
$$;
