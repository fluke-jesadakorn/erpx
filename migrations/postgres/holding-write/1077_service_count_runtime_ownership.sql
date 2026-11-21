-- Preserve retained Flow identity for source-aware runtime ownership. Existing
-- route progression updates only next_step_key/next_user_id and remains valid.
-- This migration changes no stored history and performs no data cleanup.
CREATE FUNCTION erpx_guard_flow_event_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' THEN
    RAISE EXCEPTION 'Flow event history cannot be removed'
      USING ERRCODE = '23514', CONSTRAINT = 'flow_event_identity_immutable';
  END IF;
  IF ROW(NEW.id, NEW.company_id, NEW.kind, NEW.record_id, NEW.command_id,
         NEW.action, NEW.actor_user_id, NEW.actor_name, NEW.record_version, NEW.step_key,
         NEW.reason, NEW.created_at)
     IS DISTINCT FROM
     ROW(OLD.id, OLD.company_id, OLD.kind, OLD.record_id, OLD.command_id,
         OLD.action, OLD.actor_user_id, OLD.actor_name, OLD.record_version, OLD.step_key,
         OLD.reason, OLD.created_at) THEN
    RAISE EXCEPTION 'Flow event identity cannot be rewritten'
      USING ERRCODE = '23514', CONSTRAINT = 'flow_event_identity_immutable';
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER flow_event_identity_immutable
  BEFORE UPDATE OR DELETE ON flow_events
  FOR EACH ROW EXECUTE FUNCTION erpx_guard_flow_event_identity();

-- Match the runtime ownership predicate, including malformed non-object create
-- results. Invalid JSON was already excluded by the existing JSON expression
-- index; malformed-but-valid JSON must never silently become ordinary Supply.
CREATE INDEX inventory_service_count_owned_receipt_lookup
  ON command_receipts(record_id)
  WHERE operation = 'inventory.count.create'
    AND (result_json IS NULL OR jsonb_typeof(result_json::jsonb) <> 'object'
         OR jsonb_exists(result_json::jsonb, '_serviceInitialPhysicalStockCount'));

-- A manual Flow assignment is a single-record command. Other business commands
-- may retain several transition events; they are not globally constrained to one.
-- The unique command row supplies concurrency control even when two transactions
-- began before either event was visible. No existing event is rewritten.
CREATE TABLE flow_event_command_fences (
  command_id TEXT PRIMARY KEY,
  event_count BIGINT NOT NULL CHECK (event_count > 0),
  assignment_event_id TEXT,
  CONSTRAINT flow_assignment_event_cardinality
    CHECK (assignment_event_id IS NULL OR event_count = 1)
);
INSERT INTO flow_event_command_fences(command_id,event_count,assignment_event_id)
  SELECT command_id,count(*),
    CASE WHEN count(*)=1 THEN min(id) FILTER (WHERE action='assign') ELSE NULL END
  FROM flow_events GROUP BY command_id;

CREATE FUNCTION erpx_fence_flow_event_command() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO flow_event_command_fences(command_id,event_count,assignment_event_id)
    VALUES(NEW.command_id,1,CASE WHEN NEW.action='assign' THEN NEW.id ELSE NULL END)
  ON CONFLICT(command_id) DO UPDATE SET
    event_count=flow_event_command_fences.event_count+1,
    assignment_event_id=COALESCE(flow_event_command_fences.assignment_event_id,EXCLUDED.assignment_event_id);
  RETURN NEW;
END;
$$;
CREATE TRIGGER flow_event_command_fence
  BEFORE INSERT ON flow_events
  FOR EACH ROW EXECUTE FUNCTION erpx_fence_flow_event_command();
