-- Append-only observations. Baseline is the migration observation time, never a
-- reconstructed creation time. PII is intentionally excluded.
CREATE TABLE pms_booking_observations (
 observation_id bigserial PRIMARY KEY,
 property_id text NOT NULL REFERENCES pms_properties(id),
 record_id text NOT NULL,
 kind text NOT NULL CHECK(kind IN ('reservation','stay')),
 record_version integer NOT NULL,
 observed_at timestamptz NOT NULL DEFAULT transaction_timestamp(),
 evidence text NOT NULL CHECK(evidence IN ('baseline','mutation')),
 facts jsonb NOT NULL
);
CREATE INDEX pms_booking_observation_asof ON pms_booking_observations(property_id,kind,record_id,observed_at DESC,observation_id DESC);
CREATE TABLE pms_booking_history_coverage (
 property_id text PRIMARY KEY REFERENCES pms_properties(id),
 started_at timestamptz NOT NULL DEFAULT transaction_timestamp()
);
INSERT INTO pms_booking_history_coverage(property_id) SELECT id FROM pms_properties;
CREATE FUNCTION pms_booking_history_start() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN INSERT INTO pms_booking_history_coverage(property_id) VALUES(NEW.id); RETURN NEW; END $$;
CREATE TRIGGER pms_booking_history_start AFTER INSERT ON pms_properties FOR EACH ROW EXECUTE FUNCTION pms_booking_history_start();
CREATE FUNCTION pms_booking_observation_facts(k text,d jsonb) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$
 SELECT CASE WHEN k='reservation' THEN jsonb_build_object('state',d->'state','source',d->'source')
 ELSE jsonb_build_object('reservationId',d->'reservationId','state',d->'state','arrival',d->'arrival','departure',d->'departure','prices',d->'prices') END
$$;
INSERT INTO pms_booking_observations(property_id,record_id,kind,record_version,evidence,facts)
 SELECT property_id,id,kind,version,'baseline',pms_booking_observation_facts(kind,data_json::jsonb)
 FROM pms_records WHERE kind IN ('reservation','stay');
CREATE FUNCTION pms_capture_booking_observation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.kind IN ('reservation','stay') AND (TG_OP='INSERT' OR
   pms_booking_observation_facts(NEW.kind,NEW.data_json::jsonb) IS DISTINCT FROM pms_booking_observation_facts(OLD.kind,OLD.data_json::jsonb)) THEN
  INSERT INTO pms_booking_observations(property_id,record_id,kind,record_version,evidence,facts)
   VALUES(NEW.property_id,NEW.id,NEW.kind,NEW.version,'mutation',pms_booking_observation_facts(NEW.kind,NEW.data_json::jsonb));
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER pms_capture_booking_observation AFTER INSERT OR UPDATE ON pms_records FOR EACH ROW EXECUTE FUNCTION pms_capture_booking_observation();
CREATE FUNCTION pms_booking_observation_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'PMS_BOOKING_OBSERVATION_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER pms_booking_observation_immutable BEFORE UPDATE OR DELETE ON pms_booking_observations FOR EACH ROW EXECUTE FUNCTION pms_booking_observation_immutable();
CREATE TRIGGER pms_booking_coverage_immutable BEFORE UPDATE OR DELETE ON pms_booking_history_coverage FOR EACH ROW EXECUTE FUNCTION pms_booking_observation_immutable();
