-- Seat metering begins at this cutover. Earlier IAM status is not inferred from
-- today's rows. Source IDs and access history never leave this Holding DB.
CREATE TABLE billing_seat_history_coverage (
  singleton SMALLINT PRIMARY KEY CHECK (singleton=1),
  started_at BIGINT NOT NULL
);
INSERT INTO billing_seat_history_coverage(singleton,started_at)
VALUES (1,floor(extract(epoch FROM clock_timestamp())*1000)::bigint);

CREATE TABLE billing_seat_assignment_events (
  event_id BIGSERIAL PRIMARY KEY,
  assignment_id TEXT NOT NULL,
  captured_at BIGINT NOT NULL,
  present BOOLEAN NOT NULL,
  state JSONB
);
CREATE INDEX billing_seat_assignment_events_time ON billing_seat_assignment_events(captured_at,assignment_id,event_id);
CREATE TABLE billing_seat_member_events (
  event_id BIGSERIAL PRIMARY KEY,
  company_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  captured_at BIGINT NOT NULL,
  present BOOLEAN NOT NULL,
  state JSONB
);
CREATE INDEX billing_seat_member_events_time ON billing_seat_member_events(captured_at,company_id,user_id,event_id);
CREATE TABLE billing_seat_role_events (
  event_id BIGSERIAL PRIMARY KEY,
  company_id TEXT NOT NULL,
  role_id TEXT NOT NULL,
  captured_at BIGINT NOT NULL,
  present BOOLEAN NOT NULL,
  state JSONB
);
CREATE INDEX billing_seat_role_events_time ON billing_seat_role_events(captured_at,company_id,role_id,event_id);

CREATE FUNCTION billing_seat_assignment_capture() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE row_data assignments%ROWTYPE;
BEGIN
  IF TG_OP='DELETE' THEN row_data:=OLD; ELSE row_data:=NEW; END IF;
  INSERT INTO billing_seat_assignment_events(assignment_id,captured_at,present,state)
  VALUES(row_data.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint,TG_OP<>'DELETE',
    CASE WHEN TG_OP='DELETE' THEN NULL ELSE jsonb_build_object(
      'userId',row_data.user_id,'companyId',row_data.company_id,'role',row_data.role,
      'source',row_data.source,'parentId',row_data.parent_id,'validFrom',row_data.valid_from,
      'validUntil',row_data.valid_until,'revokedAt',row_data.revoked_at) END);
  RETURN COALESCE(NEW,OLD);
END; $$;
CREATE TRIGGER billing_seat_assignments AFTER INSERT OR UPDATE OR DELETE ON assignments
FOR EACH ROW EXECUTE FUNCTION billing_seat_assignment_capture();

CREATE FUNCTION billing_seat_member_capture() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE row_data iam_members%ROWTYPE;
BEGIN
  IF TG_OP='DELETE' THEN row_data:=OLD; ELSE row_data:=NEW; END IF;
  INSERT INTO billing_seat_member_events(company_id,user_id,captured_at,present,state)
  VALUES(row_data.company_id,row_data.user_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint,
    TG_OP<>'DELETE',CASE WHEN TG_OP='DELETE' THEN NULL ELSE jsonb_build_object(
      'status',row_data.status,'loginPolicy',row_data.login_policy) END);
  RETURN COALESCE(NEW,OLD);
END; $$;
CREATE TRIGGER billing_seat_members AFTER INSERT OR UPDATE OR DELETE ON iam_members
FOR EACH ROW EXECUTE FUNCTION billing_seat_member_capture();

CREATE FUNCTION billing_seat_role_capture() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE row_data iam_roles%ROWTYPE;
BEGIN
  IF TG_OP='DELETE' THEN row_data:=OLD; ELSE row_data:=NEW; END IF;
  INSERT INTO billing_seat_role_events(company_id,role_id,captured_at,present,state)
  VALUES(row_data.company_id,row_data.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint,
    TG_OP<>'DELETE',CASE WHEN TG_OP='DELETE' THEN NULL ELSE jsonb_build_object(
      'status',row_data.status,'publishedRevision',row_data.published_revision) END);
  RETURN COALESCE(NEW,OLD);
END; $$;
CREATE TRIGGER billing_seat_roles AFTER INSERT OR UPDATE OR DELETE ON iam_roles
FOR EACH ROW EXECUTE FUNCTION billing_seat_role_capture();

-- The seed is a state at cutover, not a backdated claim about earlier access.
INSERT INTO billing_seat_assignment_events(assignment_id,captured_at,present,state)
SELECT a.id,c.started_at,true,jsonb_build_object('userId',a.user_id,'companyId',a.company_id,
  'role',a.role,'source',a.source,'parentId',a.parent_id,'validFrom',a.valid_from,
  'validUntil',a.valid_until,'revokedAt',a.revoked_at)
FROM assignments a CROSS JOIN billing_seat_history_coverage c;
INSERT INTO billing_seat_member_events(company_id,user_id,captured_at,present,state)
SELECT m.company_id,m.user_id,c.started_at,true,
  jsonb_build_object('status',m.status,'loginPolicy',m.login_policy)
FROM iam_members m CROSS JOIN billing_seat_history_coverage c;
INSERT INTO billing_seat_role_events(company_id,role_id,captured_at,present,state)
SELECT r.company_id,r.id,c.started_at,true,
  jsonb_build_object('status',r.status,'publishedRevision',r.published_revision)
FROM iam_roles r CROSS JOIN billing_seat_history_coverage c;

CREATE FUNCTION billing_seat_event_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'Seat access history is immutable'; END; $$;
CREATE TRIGGER billing_seat_assignment_immutable BEFORE UPDATE OR DELETE ON billing_seat_assignment_events
FOR EACH ROW EXECUTE FUNCTION billing_seat_event_immutable();
CREATE TRIGGER billing_seat_member_immutable BEFORE UPDATE OR DELETE ON billing_seat_member_events
FOR EACH ROW EXECUTE FUNCTION billing_seat_event_immutable();
CREATE TRIGGER billing_seat_role_immutable BEFORE UPDATE OR DELETE ON billing_seat_role_events
FOR EACH ROW EXECUTE FUNCTION billing_seat_event_immutable();
