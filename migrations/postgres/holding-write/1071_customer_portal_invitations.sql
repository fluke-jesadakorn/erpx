-- A Portal invitation authorizes enrollment for one exact approved customer
-- binding. The worker can reproduce its token from immutable facts; only the
-- token digest is stored in the Holding database.
CREATE TABLE customer_portal_invitations (
  id TEXT PRIMARY KEY,
  binding_id TEXT NOT NULL REFERENCES customer_access_bindings(id),
  holding_slug TEXT NOT NULL CHECK(holding_slug ~ '^[A-Za-z0-9_-]{1,100}$'),
  generation INTEGER NOT NULL CHECK(generation>0),
  email TEXT NOT NULL CHECK(email=lower(email)),
  token_hash TEXT NOT NULL CHECK(token_hash ~ '^[a-f0-9]{64}$'),
  token_key_version SMALLINT NOT NULL DEFAULT 1 CHECK(token_key_version=1),
  token_expires_at BIGINT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('queued','running','sent','accepted','revoked','superseded','expired','failed')),
  attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0),
  next_attempt_at BIGINT NOT NULL,
  lease_token TEXT,
  lease_until BIGINT,
  last_error TEXT,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  sent_at BIGINT,
  accepted_at BIGINT,
  accepted_user_id TEXT,
  command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id) DEFERRABLE INITIALLY DEFERRED,
  UNIQUE(binding_id,generation),
  CHECK((state='running' AND lease_token IS NOT NULL AND lease_until IS NOT NULL)
     OR (state<>'running' AND lease_token IS NULL AND lease_until IS NULL)),
  CHECK((state='accepted' AND accepted_at IS NOT NULL AND accepted_user_id IS NOT NULL)
     OR (state<>'accepted' AND accepted_at IS NULL AND accepted_user_id IS NULL))
);
CREATE UNIQUE INDEX customer_portal_one_active_invitation
  ON customer_portal_invitations(binding_id) WHERE state IN ('queued','running','sent');
CREATE INDEX customer_portal_invitation_delivery_queue
  ON customer_portal_invitations(state,next_attempt_at,created_at,id);
CREATE TABLE customer_portal_invitation_events (
  id TEXT PRIMARY KEY,
  invitation_id TEXT NOT NULL REFERENCES customer_portal_invitations(id),
  action TEXT NOT NULL CHECK(action IN ('issued','delivery-started','delivery-sent','delivery-retried','delivery-failed','accepted','revoked','superseded','expired')),
  actor_id TEXT NOT NULL,
  reason TEXT NOT NULL DEFAULT '',
  created_at BIGINT NOT NULL
);
CREATE INDEX customer_portal_invitation_event_history
  ON customer_portal_invitation_events(invitation_id,created_at,id);
CREATE TABLE customer_portal_enrollment_attempts (
  invitation_id TEXT PRIMARY KEY REFERENCES customer_portal_invitations(id),
  window_start BIGINT NOT NULL,
  attempts INTEGER NOT NULL CHECK(attempts>0)
);

CREATE FUNCTION customer_portal_invitation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_INVITATION_IMMUTABLE' USING ERRCODE='23514'; END IF;
  IF (NEW.id,NEW.binding_id,NEW.holding_slug,NEW.generation,NEW.email,NEW.token_hash,NEW.token_key_version,NEW.token_expires_at,NEW.created_by,NEW.created_at,NEW.command_id)
      IS DISTINCT FROM
     (OLD.id,OLD.binding_id,OLD.holding_slug,OLD.generation,OLD.email,OLD.token_hash,OLD.token_key_version,OLD.token_expires_at,OLD.created_by,OLD.created_at,OLD.command_id)
  THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_INVITATION_SCOPE_IMMUTABLE' USING ERRCODE='23514'; END IF;
  IF NEW.state IS DISTINCT FROM OLD.state AND NOT (
       (OLD.state='queued' AND NEW.state IN ('running','revoked','superseded','expired','failed'))
    OR (OLD.state='running' AND NEW.state IN ('queued','sent','accepted','revoked','superseded','expired','failed'))
    OR (OLD.state='sent' AND NEW.state IN ('accepted','revoked','superseded','expired','failed'))
  ) THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_INVITATION_TRANSITION_INVALID' USING ERRCODE='23514'; END IF;
  IF OLD.state IN ('accepted','revoked','superseded','expired','failed') AND NEW IS DISTINCT FROM OLD
  THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_INVITATION_TERMINAL' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER customer_portal_invitation_guard BEFORE UPDATE OR DELETE ON customer_portal_invitations FOR EACH ROW EXECUTE FUNCTION customer_portal_invitation_guard();
CREATE FUNCTION customer_portal_invitation_event_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'CUSTOMER_PORTAL_INVITATION_EVENT_IMMUTABLE' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER customer_portal_invitation_event_guard BEFORE UPDATE OR DELETE ON customer_portal_invitation_events FOR EACH ROW EXECUTE FUNCTION customer_portal_invitation_event_guard();

INSERT INTO work_cache_revisions(table_name) VALUES('customer_portal_invitations'),('customer_portal_invitation_events'),('customer_portal_enrollment_attempts') ON CONFLICT DO NOTHING;
CREATE TRIGGER customer_portal_invitation_work_cache AFTER INSERT OR UPDATE OR DELETE ON customer_portal_invitations FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER customer_portal_invitation_event_work_cache AFTER INSERT ON customer_portal_invitation_events FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER customer_portal_enrollment_attempt_work_cache AFTER INSERT OR UPDATE OR DELETE ON customer_portal_enrollment_attempts FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
