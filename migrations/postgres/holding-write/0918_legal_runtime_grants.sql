-- Keep applied Legal migrations immutable. Installed runtime identities need
-- explicit grants when their schema predates provisioning default privileges.
DO $$
DECLARE app_role TEXT; jobs_role TEXT; runtime_role TEXT;
BEGIN
  IF current_user !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(current_user, '_migration$', '_app');
  jobs_role := regexp_replace(current_user, '_migration$', '_jobs');
  FOREACH runtime_role IN ARRAY ARRAY[app_role,jobs_role] LOOP
    IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=runtime_role) THEN
      EXECUTE format('GRANT SELECT ON company_legal_release_revisions,legal_runtime_health,legal_executed_copies,legal_executed_reviews,legal_law_sources,legal_law_revisions,legal_law_applicability,legal_law_source_checks,legal_source_chunks,legal_text_reviews TO %I',runtime_role);
    END IF;
  END LOOP;
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
    EXECUTE format('GRANT INSERT,UPDATE ON company_legal_release_revisions,legal_executed_copies,legal_law_sources,legal_law_revisions,legal_law_applicability,legal_text_reviews TO %I',app_role);
    EXECUTE format('GRANT INSERT ON legal_executed_reviews,legal_source_chunks TO %I',app_role);
    EXECUTE format('GRANT UPDATE ON legal_law_source_checks TO %I',app_role);
  END IF;
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=jobs_role) THEN
    EXECUTE format('GRANT INSERT,UPDATE ON legal_runtime_health,legal_law_source_checks TO %I',jobs_role);
  END IF;
END;
$$;
