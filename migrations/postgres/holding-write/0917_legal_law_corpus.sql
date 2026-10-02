-- Official sources and extracted text remain pending until independently reviewed.
-- No migration installs legal conclusions or changes existing deadlines.
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE TABLE legal_law_sources (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,rule_id TEXT NOT NULL,
  title TEXT NOT NULL,source_url TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','disabled')),
  version INTEGER NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,
  UNIQUE(company_id,id),UNIQUE(rule_id,source_url),
  FOREIGN KEY(company_id,rule_id) REFERENCES legal_items(company_id,id)
);
CREATE TABLE legal_law_revisions (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,rule_id TEXT NOT NULL,source_id TEXT NOT NULL,
  document_id TEXT NOT NULL REFERENCES legal_documents(id),document_checksum TEXT NOT NULL,
  revision_label TEXT NOT NULL,published_on TEXT NOT NULL,effective_from TEXT NOT NULL,effective_until TEXT,
  extraction_method TEXT NOT NULL CHECK(extraction_method IN ('manual','ocr')),inspection_id TEXT REFERENCES legal_document_inspections(id),
  state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','approved','retired')),
  version INTEGER NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,
  reviewed_by TEXT,reviewed_at BIGINT,review_reason TEXT,
  UNIQUE(company_id,id),UNIQUE(source_id,document_checksum,revision_label),
  FOREIGN KEY(company_id,rule_id) REFERENCES legal_items(company_id,id),
  FOREIGN KEY(company_id,source_id) REFERENCES legal_law_sources(company_id,id),
  CHECK(effective_until IS NULL OR effective_until>effective_from)
);
CREATE TABLE legal_law_applicability (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,rule_id TEXT NOT NULL,revision_id TEXT NOT NULL,
  applies BOOLEAN NOT NULL,basis TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','approved')),
  version INTEGER NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,reviewed_by TEXT,reviewed_at BIGINT,
  UNIQUE(company_id,id),FOREIGN KEY(company_id,revision_id) REFERENCES legal_law_revisions(company_id,id),
  FOREIGN KEY(company_id,rule_id) REFERENCES legal_items(company_id,id)
);
CREATE INDEX legal_law_applicability_latest ON legal_law_applicability(company_id,revision_id,created_at DESC,id DESC) WHERE state='approved';
CREATE TABLE legal_text_reviews (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,item_id TEXT NOT NULL,document_id TEXT NOT NULL REFERENCES legal_documents(id),
  document_checksum TEXT NOT NULL,extraction_method TEXT NOT NULL CHECK(extraction_method IN ('manual','ocr')),
  inspection_id TEXT REFERENCES legal_document_inspections(id),state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','approved')),
  version INTEGER NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,reviewed_by TEXT,reviewed_at BIGINT,review_reason TEXT,
  UNIQUE(company_id,id),FOREIGN KEY(company_id,item_id) REFERENCES legal_items(company_id,id)
);
CREATE TABLE legal_source_chunks (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,item_id TEXT NOT NULL,document_id TEXT NOT NULL REFERENCES legal_documents(id),
  document_checksum TEXT NOT NULL,revision_id TEXT,text_review_id TEXT,page INTEGER NOT NULL CHECK(page>0),
  page_mapping BOOLEAN NOT NULL DEFAULT true,section TEXT NOT NULL DEFAULT '',text TEXT NOT NULL CHECK(length(text)>0),text_checksum TEXT NOT NULL,
  search TSVECTOR GENERATED ALWAYS AS (to_tsvector('simple'::regconfig,text)) STORED,
  FOREIGN KEY(company_id,item_id) REFERENCES legal_items(company_id,id),
  FOREIGN KEY(company_id,revision_id) REFERENCES legal_law_revisions(company_id,id),
  FOREIGN KEY(company_id,text_review_id) REFERENCES legal_text_reviews(company_id,id),
  CHECK((revision_id IS NOT NULL)::int+(text_review_id IS NOT NULL)::int=1)
);
CREATE INDEX legal_source_chunks_fulltext ON legal_source_chunks USING GIN(search);
CREATE INDEX legal_source_chunks_trigram ON legal_source_chunks USING GIN(text gin_trgm_ops);
CREATE INDEX legal_source_chunks_scope ON legal_source_chunks(company_id,item_id,document_id);
CREATE TABLE legal_law_source_checks (
  id TEXT PRIMARY KEY,source_id TEXT NOT NULL REFERENCES legal_law_sources(id),company_id TEXT NOT NULL,rule_id TEXT NOT NULL,
  check_date TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','unchanged','pending_review','reviewed','failed')),
  attempts INTEGER NOT NULL DEFAULT 0,lease_token TEXT,lease_until BIGINT,checked_at BIGINT,
  checksum TEXT,object_key TEXT,error TEXT,candidate_revision_id TEXT REFERENCES legal_law_revisions(id),created_at BIGINT NOT NULL,
  UNIQUE(source_id,check_date),FOREIGN KEY(company_id,source_id) REFERENCES legal_law_sources(company_id,id)
);
CREATE INDEX legal_law_source_checks_queue ON legal_law_source_checks(state,created_at);
CREATE FUNCTION legal_corpus_revision_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'LEGAL_CORPUS_PRESERVATION_REQUIRED'; END IF;
  IF (to_jsonb(NEW)-ARRAY['state','version','reviewed_by','reviewed_at','review_reason']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['state','version','reviewed_by','reviewed_at','review_reason']) THEN RAISE EXCEPTION 'LEGAL_CORPUS_REVISION_IMMUTABLE'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER legal_law_revisions_immutable BEFORE UPDATE OR DELETE ON legal_law_revisions FOR EACH ROW EXECUTE FUNCTION legal_corpus_revision_immutable();
CREATE TRIGGER legal_text_reviews_immutable BEFORE UPDATE OR DELETE ON legal_text_reviews FOR EACH ROW EXECUTE FUNCTION legal_corpus_revision_immutable();
CREATE TRIGGER legal_law_applicability_immutable BEFORE UPDATE OR DELETE ON legal_law_applicability FOR EACH ROW EXECUTE FUNCTION legal_corpus_revision_immutable();
CREATE TRIGGER legal_source_chunks_immutable BEFORE UPDATE OR DELETE ON legal_source_chunks FOR EACH ROW EXECUTE FUNCTION legal_evidence_immutable();
CREATE FUNCTION legal_corpus_configuration_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE before_data JSONB; after_data JSONB; company TEXT;
BEGIN
  before_data := CASE WHEN TG_OP='INSERT' THEN '{}'::jsonb ELSE to_jsonb(OLD) END;
  after_data := CASE WHEN TG_OP='DELETE' THEN '{}'::jsonb ELSE to_jsonb(NEW) END;
  IF TG_TABLE_NAME='legal_law_sources' THEN
    IF NOT EXISTS(SELECT 1 FROM legal_law_revisions WHERE source_id=COALESCE(after_data->>'id',before_data->>'id') AND state='approved') THEN RETURN NULL; END IF;
  ELSIF COALESCE(before_data->>'state','pending')<>'approved' AND COALESCE(after_data->>'state','pending')<>'approved' THEN RETURN NULL;
  END IF;
  IF TG_OP='UPDATE' AND before_data=after_data THEN RETURN NULL; END IF;
  company := COALESCE(after_data->>'company_id',before_data->>'company_id');
  INSERT INTO company_legal_release_revisions(company_id,revision) VALUES(company,1)
    ON CONFLICT(company_id) DO UPDATE SET revision=company_legal_release_revisions.revision+1;
  RETURN NULL;
END $$;
DO $$ DECLARE tab TEXT; BEGIN
  FOREACH tab IN ARRAY ARRAY['legal_law_sources','legal_law_revisions','legal_law_applicability'] LOOP
    EXECUTE format('CREATE TRIGGER legal_configuration_changed AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION legal_corpus_configuration_changed()',tab);
  END LOOP;
END $$;
