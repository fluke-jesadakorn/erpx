-- Disposable, active-generation Learning, Benefits and Field Service projections.
CREATE SCHEMA IF NOT EXISTS erpx_learning_read;
CREATE SCHEMA IF NOT EXISTS erpx_benefits_read;
CREATE SCHEMA IF NOT EXISTS erpx_field_service_read;

CREATE OR REPLACE FUNCTION erpx_learning_read.current_tenant_id() RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.tenant_id', true), '')::uuid $$;
CREATE OR REPLACE FUNCTION erpx_benefits_read.current_tenant_id() RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.tenant_id', true), '')::uuid $$;
CREATE OR REPLACE FUNCTION erpx_field_service_read.current_tenant_id() RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.tenant_id', true), '')::uuid $$;

CREATE TABLE erpx_learning_read.enrollment_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_learning_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'learning.enrollments' CHECK (projection_name = 'learning.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, enrollment_id uuid NOT NULL, authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,enrollment_id), UNIQUE (tenant_id,projection_name,generation,enrollment_id),
  FOREIGN KEY (tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_learning_read.completion_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_learning_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'learning.enrollments' CHECK (projection_name = 'learning.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, enrollment_id uuid NOT NULL, employee_record_id uuid NOT NULL, course_id uuid NOT NULL, course_version bigint NOT NULL CHECK (course_version > 0),
  completed_at timestamptz NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0),
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,enrollment_id),
  FOREIGN KEY (tenant_id,projection_name,generation,enrollment_id) REFERENCES erpx_learning_read.enrollment_projection(tenant_id,projection_name,generation,enrollment_id) ON DELETE CASCADE
);
CREATE TABLE erpx_learning_read.governed_record_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_learning_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'learning.enrollments' CHECK (projection_name = 'learning.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK (record_kind IN ('assignment','completion')),
  authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id), UNIQUE (tenant_id,projection_name,generation,record_id),
  FOREIGN KEY (tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_benefits_read.enrollment_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_benefits_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'benefits.enrollments' CHECK (projection_name = 'benefits.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, enrollment_id uuid NOT NULL, authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,enrollment_id), UNIQUE (tenant_id,projection_name,generation,enrollment_id),
  FOREIGN KEY (tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_benefits_read.contribution_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_benefits_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'benefits.enrollments' CHECK (projection_name = 'benefits.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, enrollment_id uuid NOT NULL, employee_minor bigint NOT NULL, employer_minor bigint NOT NULL, currency char(3) NOT NULL,
  provider_acknowledged boolean NOT NULL, payroll_handed_off boolean NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0),
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,enrollment_id),
  FOREIGN KEY (tenant_id,projection_name,generation,enrollment_id) REFERENCES erpx_benefits_read.enrollment_projection(tenant_id,projection_name,generation,enrollment_id) ON DELETE CASCADE
);
CREATE TABLE erpx_benefits_read.governed_record_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_benefits_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'benefits.enrollments' CHECK (projection_name = 'benefits.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK (record_kind IN ('window','handoff')),
  authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id), UNIQUE (tenant_id,projection_name,generation,record_id),
  FOREIGN KEY (tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_field_service_read.order_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_field_service_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'field_service.orders' CHECK (projection_name = 'field_service.orders'),
  generation uuid NOT NULL, partition_key text NOT NULL, service_order_id uuid NOT NULL, authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,service_order_id), UNIQUE (tenant_id,projection_name,generation,service_order_id),
  FOREIGN KEY (tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_field_service_read.dispatch_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_field_service_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'field_service.orders' CHECK (projection_name = 'field_service.orders'),
  generation uuid NOT NULL, partition_key text NOT NULL, service_order_id uuid NOT NULL, technician_record_id uuid, scheduled_on date,
  coverage_valid boolean NOT NULL, skills_valid boolean NOT NULL, inventory_valid boolean NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0),
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,service_order_id),
  FOREIGN KEY (tenant_id,projection_name,generation,service_order_id) REFERENCES erpx_field_service_read.order_projection(tenant_id,projection_name,generation,service_order_id) ON DELETE CASCADE
);
CREATE TABLE erpx_field_service_read.exception_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_field_service_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'field_service.orders' CHECK (projection_name = 'field_service.orders'),
  generation uuid NOT NULL, partition_key text NOT NULL, exception_id uuid NOT NULL, service_order_id uuid NOT NULL, exception_kind text NOT NULL, reason text NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0),
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,exception_id),
  FOREIGN KEY (tenant_id,projection_name,generation,service_order_id) REFERENCES erpx_field_service_read.order_projection(tenant_id,projection_name,generation,service_order_id) ON DELETE CASCADE
);
CREATE TABLE erpx_field_service_read.governed_record_projection (
  tenant_id uuid NOT NULL DEFAULT erpx_field_service_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'field_service.orders' CHECK (projection_name = 'field_service.orders'),
  generation uuid NOT NULL, partition_key text NOT NULL, record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK (record_kind IN ('request','dispatch','visit','handoff')),
  authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id,projection_name,generation,partition_key,record_id), UNIQUE (tenant_id,projection_name,generation,record_id),
  FOREIGN KEY (tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);

DO $$
DECLARE relation_name text;
BEGIN
  FOREACH relation_name IN ARRAY ARRAY[
    'erpx_learning_read.enrollment_projection','erpx_learning_read.completion_projection','erpx_learning_read.governed_record_projection',
    'erpx_benefits_read.enrollment_projection','erpx_benefits_read.contribution_projection','erpx_benefits_read.governed_record_projection',
    'erpx_field_service_read.order_projection','erpx_field_service_read.dispatch_projection','erpx_field_service_read.exception_projection','erpx_field_service_read.governed_record_projection'
  ] LOOP
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('CREATE POLICY tenant_isolation ON %s USING (tenant_id = nullif(current_setting(''app.tenant_id'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''app.tenant_id'', true), '''')::uuid)', relation_name);
  END LOOP;
END $$;
