-- Legal has its own release cut. Existing hotel attestations stay intact.
CREATE TABLE company_legal_release_revisions (
  company_id TEXT PRIMARY KEY REFERENCES organization_nodes(id),
  revision BIGINT NOT NULL DEFAULT 0 CHECK(revision >= 0)
);
INSERT INTO company_legal_release_revisions(company_id)
  SELECT id FROM organization_nodes WHERE kind='company' ON CONFLICT DO NOTHING;
CREATE INDEX company_release_acceptances_scope_current
  ON company_release_acceptances(company_id,scope_version,created_at DESC,id DESC);

CREATE FUNCTION invalidate_legal_release() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE before_data JSONB; after_data JSONB; company TEXT; item TEXT;
BEGIN
  before_data := CASE WHEN TG_OP='INSERT' THEN '{}'::jsonb ELSE to_jsonb(OLD) END;
  after_data := CASE WHEN TG_OP='DELETE' THEN '{}'::jsonb ELSE to_jsonb(NEW) END;
  IF TG_OP='UPDATE' AND before_data=after_data THEN RETURN NULL; END IF;
  IF TG_TABLE_NAME='legal_items' THEN
    IF COALESCE(after_data->>'kind',before_data->>'kind') NOT IN ('rule','template') THEN RETURN NULL; END IF;
    IF TG_OP='UPDATE' AND (after_data-ARRAY['version','updated_at'])=(before_data-ARRAY['version','updated_at']) THEN RETURN NULL; END IF;
  END IF;
  company := COALESCE(after_data->>'company_id',before_data->>'company_id');
  IF company IS NULL THEN
    item := COALESCE(after_data->>'item_id',before_data->>'item_id');
    SELECT company_id INTO company FROM legal_items WHERE id=item;
  END IF;
  IF company IS NOT NULL THEN
    INSERT INTO company_legal_release_revisions(company_id,revision) VALUES(company,1)
      ON CONFLICT(company_id) DO UPDATE SET revision=company_legal_release_revisions.revision+1;
  END IF;
  RETURN NULL;
END $$;
DO $$ DECLARE tab TEXT; BEGIN
  FOREACH tab IN ARRAY ARRAY['organization_nodes','company_business_acceptance',
    'legal_settings','legal_items','legal_rules','legal_templates','legal_retention_policies','ai_connections','ai_company_settings','ai_gateway_policies'] LOOP
    IF to_regclass(tab) IS NOT NULL THEN
      EXECUTE format('CREATE TRIGGER legal_configuration_changed AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION invalidate_legal_release()',tab);
    END IF;
  END LOOP;
END $$;

-- Worker failures remain inspectable per Holding and task, without document text.
CREATE TABLE legal_runtime_health (
  task TEXT PRIMARY KEY,
  heartbeat_at BIGINT NOT NULL,
  last_success_at BIGINT,
  failure_count INTEGER NOT NULL DEFAULT 0,
  last_error TEXT
);
