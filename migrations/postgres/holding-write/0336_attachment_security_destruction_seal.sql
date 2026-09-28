-- Malware and scan failures remain sealed until a separately approved security
-- destruction policy is implemented. Generic retention cleanup must never be
-- able to turn these terminal security states into physical deletion.
CREATE OR REPLACE FUNCTION attachment_security_destruction_seal() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.source_state IN ('scan_error','malware_rejected') AND NEW.source_state='destroyed' THEN
    RAISE EXCEPTION 'ATTACHMENT_SECURITY_DESTRUCTION_POLICY_REQUIRED';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER attachment_security_destruction_seal
  BEFORE UPDATE OF source_state ON attachments
  FOR EACH ROW EXECUTE FUNCTION attachment_security_destruction_seal();
