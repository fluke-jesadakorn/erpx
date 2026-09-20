-- Freeze record metadata as well as documents for every new audited event.
-- Existing evidence is never rewritten or backfilled with guessed history.
ALTER TABLE legal_events ADD COLUMN snapshot JSONB;
CREATE FUNCTION legal_event_snapshot() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item legal_items%ROWTYPE; payload jsonb; source_table text;
BEGIN
 SELECT * INTO STRICT item FROM legal_items WHERE id=NEW.item_id;
 source_table := CASE item.kind
 WHEN 'request' THEN 'legal_requests' WHEN 'matter' THEN 'legal_matters'
 WHEN 'contract' THEN 'legal_contracts' WHEN 'obligation' THEN 'legal_obligations'
 WHEN 'compliance' THEN 'legal_compliance' WHEN 'license' THEN 'legal_licenses'
 WHEN 'corporate' THEN 'legal_corporate' WHEN 'ip' THEN 'legal_ip_assets'
 WHEN 'privacy' THEN 'legal_privacy' WHEN 'counsel' THEN 'legal_counsel'
 WHEN 'template' THEN 'legal_templates' WHEN 'rule' THEN 'legal_rules' END;
 EXECUTE format('SELECT data FROM %I WHERE item_id=$1',source_table) INTO payload USING item.id;
 NEW.snapshot := to_jsonb(item) || jsonb_build_object('data',coalesce(payload,'{}'::jsonb));
 RETURN NEW;
END $$;
CREATE TRIGGER legal_event_snapshot BEFORE INSERT ON legal_events FOR EACH ROW EXECUTE FUNCTION legal_event_snapshot();
