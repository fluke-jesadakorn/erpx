-- Field evidence access is recorded on its selected native source transaction.
-- Other Executive providers and every retained audit row remain unchanged.
CREATE FUNCTION guard_native_field_evidence_access() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE marker jsonb;
BEGIN
 IF NEW.provider_id<>'field-service-orders' OR NEW.metric_id<>'field-visit-completion' THEN RETURN NEW; END IF;
 BEGIN
  marker:=current_setting('erpx.native_field_evidence',true)::jsonb;
 EXCEPTION WHEN invalid_text_representation THEN marker:=NULL;
 END;
 IF marker IS NULL OR marker->>'unit' IS DISTINCT FROM pg_current_xact_id()::text
  OR jsonb_typeof(marker->'rows') IS DISTINCT FROM 'array'
  OR NOT(marker->'rows' @> jsonb_build_array(to_jsonb(NEW))) THEN
  RAISE EXCEPTION 'NATIVE_FIELD_EVIDENCE_REQUIRED' USING ERRCODE='23514',CONSTRAINT='native_field_evidence_required';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_field_evidence_access BEFORE INSERT ON executive_evidence_access_events
 FOR EACH ROW EXECUTE FUNCTION guard_native_field_evidence_access();
