-- Configured Account environments are exact labels bound by Account/Holding
-- audiences, not a development-only literal. This replaces only the three
-- known inline environment checks with a bounded label check. Existing
-- development rows remain valid; no session metadata is rewritten.
DO $$
DECLARE
  target text;
BEGIN
  FOREACH target IN ARRAY ARRAY[
    'sso_session_bindings',
    'sso_browser_transactions',
    'sso_authorization_grants'
  ] LOOP
    IF to_regclass('public.' || target) IS NULL THEN
      RAISE EXCEPTION '0044 requires table %', target;
    END IF;
    IF NOT EXISTS (
      SELECT 1
      FROM pg_constraint
      WHERE conrelid = ('public.' || target)::regclass
        AND conname = target || '_environment_bounded'
    ) THEN
      EXECUTE format(
        'ALTER TABLE public.%I DROP CONSTRAINT IF EXISTS %I',
        target,
        target || '_environment_check'
      );
      EXECUTE format(
        'ALTER TABLE public.%I ADD CONSTRAINT %I CHECK (environment ~ ''^[a-z0-9-]{1,64}$'')',
        target,
        target || '_environment_bounded'
      );
    END IF;
  END LOOP;
END $$;
