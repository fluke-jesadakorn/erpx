-- Existing reviewed identity links retain identity evidence, but history sharing
-- begins only after explicit authority evidence; no legacy consent is invented.
ALTER TABLE crm_guest_links ADD COLUMN sharing_state text NOT NULL DEFAULT 'pending' CHECK(sharing_state IN ('pending','granted','revoked'));
ALTER TABLE crm_guest_links ADD COLUMN sharing_evidence text NOT NULL DEFAULT '';
ALTER TABLE crm_guest_links ADD COLUMN sharing_by text;
ALTER TABLE crm_guest_links ADD COLUMN sharing_at bigint;
ALTER TABLE crm_guest_links ADD COLUMN version integer NOT NULL DEFAULT 1;
CREATE TABLE crm_guest_sharing_events (
 id text PRIMARY KEY,
 link_id text NOT NULL REFERENCES crm_guest_links(id),
 company_id text NOT NULL,
 state text NOT NULL CHECK(state IN ('granted','revoked')),
 evidence text NOT NULL CHECK(length(trim(evidence))>=3),
 actor_id text NOT NULL,
 version integer NOT NULL,
 created_at bigint NOT NULL,
 UNIQUE(link_id,version)
);
CREATE FUNCTION crm_guest_sharing_event_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'CRM_GUEST_SHARING_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER crm_guest_sharing_event_immutable BEFORE UPDATE OR DELETE ON crm_guest_sharing_events FOR EACH ROW EXECUTE FUNCTION crm_guest_sharing_event_immutable();
