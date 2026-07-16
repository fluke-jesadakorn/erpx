-- Read-only role used by chat-to-SQL. Cannot INSERT/UPDATE/DELETE/TRUNCATE
-- on any folio, finance, hr, or public table.

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'folio_readonly_agent') THEN
    CREATE ROLE folio_readonly_agent LOGIN PASSWORD 'agent_readonly_pw_change_me';
  END IF;
END $$;

GRANT CONNECT ON DATABASE folio_db TO folio_readonly_agent;

DO $$ BEGIN
  BEGIN
    GRANT USAGE ON SCHEMA folio TO folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'grant usage on folio skipped: %', SQLERRM;
  END;
  BEGIN
    GRANT USAGE ON SCHEMA finance TO folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'grant usage on finance skipped: %', SQLERRM;
  END;
  BEGIN
    GRANT USAGE ON SCHEMA hr TO folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'grant usage on hr skipped: %', SQLERRM;
  END;
  BEGIN
    GRANT USAGE ON SCHEMA public TO folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'grant usage on public skipped: %', SQLERRM;
  END;
END $$;

DO $$ BEGIN
  BEGIN
    GRANT SELECT ON ALL TABLES IN SCHEMA folio TO folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'grant select folio skipped: %', SQLERRM;
  END;
  BEGIN
    GRANT SELECT ON ALL TABLES IN SCHEMA finance TO folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'grant select finance skipped: %', SQLERRM;
  END;
  BEGIN
    GRANT SELECT ON ALL TABLES IN SCHEMA hr TO folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'grant select hr skipped: %', SQLERRM;
  END;
  BEGIN
    GRANT SELECT ON ALL TABLES IN SCHEMA public TO folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'grant select public skipped: %', SQLERRM;
  END;
END $$;

DO $$ BEGIN
  BEGIN
    REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER
      ON ALL TABLES IN SCHEMA folio FROM folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'revoke write folio skipped: %', SQLERRM;
  END;
  BEGIN
    REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER
      ON ALL TABLES IN SCHEMA finance FROM folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'revoke write finance skipped: %', SQLERRM;
  END;
  BEGIN
    REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER
      ON ALL TABLES IN SCHEMA hr FROM folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'revoke write hr skipped: %', SQLERRM;
  END;
  BEGIN
    REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER
      ON ALL TABLES IN SCHEMA public FROM folio_readonly_agent;
  EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'revoke write public skipped: %', SQLERRM;
  END;
END $$;

ALTER DEFAULT PRIVILEGES IN SCHEMA folio
  GRANT SELECT ON TABLES TO folio_readonly_agent;
ALTER DEFAULT PRIVILEGES IN SCHEMA finance
  GRANT SELECT ON TABLES TO folio_readonly_agent;
ALTER DEFAULT PRIVILEGES IN SCHEMA hr
  GRANT SELECT ON TABLES TO folio_readonly_agent;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
  GRANT SELECT ON TABLES TO folio_readonly_agent;