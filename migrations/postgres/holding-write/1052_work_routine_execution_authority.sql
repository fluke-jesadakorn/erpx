-- Legacy schedules and their immutable journals remain untouched. Only an
-- explicit native create/update/resume captures its original current actor.
CREATE TABLE work_routine_execution_authorizations (
 command_id TEXT PRIMARY KEY REFERENCES command_receipts(command_id) DEFERRABLE INITIALLY DEFERRED,
 schedule_id TEXT NOT NULL REFERENCES routine_schedules(id),
 template_id TEXT NOT NULL REFERENCES routine_templates(id),
 company_id TEXT NOT NULL REFERENCES organization_nodes(id),
 kind TEXT NOT NULL CHECK(kind IN ('create','update','resume')),
 actor_id TEXT NOT NULL,
 assignment_id TEXT NOT NULL,
 authority_json TEXT NOT NULL CHECK(erpx_json_valid(authority_json)),
 source_hash TEXT NOT NULL CHECK(source_hash ~ '^[a-f0-9]{64}$'),
 template_version BIGINT NOT NULL CHECK(template_version>0),
 schedule_version BIGINT NOT NULL CHECK(schedule_version>0),
 initial_occurrence_at BIGINT,
 created_at BIGINT NOT NULL
);
CREATE INDEX work_routine_execution_authorizations_source ON work_routine_execution_authorizations(schedule_id,schedule_version DESC,created_at DESC,command_id);
-- Every cursor advance under an authorization has a contiguous immutable
-- owner step, including an original paused/prior occurrence. Generated tasks
-- retain their existing audit/outbox journal rather than a parallel workflow.
CREATE TABLE work_routine_execution_steps (
 command_id TEXT PRIMARY KEY,
 authorization_command_id TEXT NOT NULL REFERENCES work_routine_execution_authorizations(command_id),
 schedule_id TEXT NOT NULL REFERENCES routine_schedules(id),
 company_id TEXT NOT NULL REFERENCES organization_nodes(id),
 occurrence_at BIGINT NOT NULL,
 next_occurrence_at BIGINT,
 task_id TEXT REFERENCES staff_tasks(id),
 generated BOOLEAN NOT NULL,
 source_hash TEXT NOT NULL CHECK(source_hash ~ '^[a-f0-9]{64}$'),
 effects_hash TEXT NOT NULL CHECK(effects_hash ~ '^[a-f0-9]{64}$'),
 created_at BIGINT NOT NULL,
 UNIQUE(authorization_command_id,occurrence_at)
);
CREATE FUNCTION work_routine_execution_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'WORK_ROUTINE_EXECUTION_IMMUTABLE' USING ERRCODE='23514';END $$;
CREATE TRIGGER work_routine_execution_authorization_immutable BEFORE UPDATE OR DELETE ON work_routine_execution_authorizations FOR EACH ROW EXECUTE FUNCTION work_routine_execution_immutable();
CREATE TRIGGER work_routine_execution_step_immutable BEFORE UPDATE OR DELETE ON work_routine_execution_steps FOR EACH ROW EXECUTE FUNCTION work_routine_execution_immutable();
