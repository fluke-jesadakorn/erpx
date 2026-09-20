CREATE FUNCTION legal_signer_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR OLD.state IN ('signed','declined') THEN RAISE EXCEPTION 'LEGAL_SIGNATURE_EVIDENCE_IMMUTABLE'; END IF;
 IF NEW.envelope_id<>OLD.envelope_id OR NEW.user_id<>OLD.user_id OR NEW.ordinal<>OLD.ordinal THEN RAISE EXCEPTION 'LEGAL_SIGNER_SET_IMMUTABLE'; END IF;
 RETURN NEW; END $$;
CREATE TRIGGER legal_signer_immutable BEFORE UPDATE OR DELETE ON legal_signers FOR EACH ROW EXECUTE FUNCTION legal_signer_immutable();
CREATE TRIGGER legal_approval_immutable BEFORE UPDATE OR DELETE ON legal_approvals FOR EACH ROW EXECUTE FUNCTION legal_evidence_immutable();
CREATE TRIGGER legal_external_upload_immutable BEFORE UPDATE OR DELETE ON legal_external_uploads FOR EACH ROW EXECUTE FUNCTION legal_evidence_immutable();
CREATE FUNCTION legal_destruction_manifest_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (to_jsonb(NEW)-ARRAY['state','approved_by','approved_at','completed_at','version','error','started_at','attempts']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['state','approved_by','approved_at','completed_at','version','error','started_at','attempts']) THEN RAISE EXCEPTION 'LEGAL_DESTRUCTION_MANIFEST_IMMUTABLE'; END IF;
 IF OLD.state='completed' THEN RAISE EXCEPTION 'LEGAL_DESTRUCTION_CERTIFICATE_IMMUTABLE'; END IF;
 RETURN NEW; END $$;
CREATE TRIGGER legal_destruction_manifest_immutable BEFORE UPDATE OR DELETE ON legal_destructions FOR EACH ROW EXECUTE FUNCTION legal_destruction_manifest_immutable();
