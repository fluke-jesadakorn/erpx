-- Configured Account environments are exact labels bound by Account/Holding
-- audiences, not a development-only literal. Existing development rows remain
-- valid and no session metadata is rewritten or discarded.
DO $$
DECLARE
  target text;
  constraint_name text;
BEGIN
  FOREACH target IN ARRAY ARRAY[
    'sso_session_bindings',
    'sso_browser_transactions',
    'sso_authorization_grants'
  ] LOOP
    FOR constraint_name IN
      SELECT conname
      FROM pg_constraint
      WHERE conrelid = target::regclass
        AND contype = 'c'
        AND pg_get_constraintdef(oid) LIKE '%environment%'
    LOOP
      EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I', target, constraint_name);
    END LOOP;
    EXECUTE format(
      'ALTER TABLE %I ADD CONSTRAINT %I CHECK (environment ~ ''^[a-z0-9-]{1,64}$'')',
      target,
      target || '_environment_bounded'
    );
  END LOOP;
END $$;
