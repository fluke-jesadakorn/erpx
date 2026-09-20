-- New provisioning records participate in the same transactional cache fence.
DO $$ DECLARE name text;
BEGIN
 FOREACH name IN ARRAY ARRAY['people_provision_operations','people_applied_packages','people_provision_grants','people_directory_membership_sources','page_layouts'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(name);
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',name);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',name);
 END LOOP;
END $$;
-- A revoked ancestor of the authorizing IT assignment still revokes derived access.
DO $$ DECLARE body text;
BEGIN
 body:=pg_get_functiondef('people_application_active(text,bigint)'::regprocedure);
 body:=replace(body,'a.revoked_at IS NULL AND a.valid_from<=at_time AND a.valid_until>at_time AND iam_principal_active(a.user_id,a.company_id,at_time)','iam_assignment_active(a.id,at_time)');
 EXECUTE body;
END $$;
