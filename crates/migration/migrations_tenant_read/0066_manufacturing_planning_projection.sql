-- Disposable, active-generation Manufacturing and Planning projections.
CREATE SCHEMA IF NOT EXISTS erpx_manufacturing_read;
CREATE SCHEMA IF NOT EXISTS erpx_planning_read;

CREATE OR REPLACE FUNCTION erpx_manufacturing_read.current_tenant_id()
RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.tenant_id', true), '')::uuid $$;
CREATE OR REPLACE FUNCTION erpx_planning_read.current_tenant_id()
RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.tenant_id', true), '')::uuid $$;

CREATE TABLE erpx_manufacturing_read.order_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_tenant_id(),
    projection_name text NOT NULL DEFAULT 'manufacturing.orders' CHECK (projection_name = 'manufacturing.orders'),
    generation uuid NOT NULL, partition_key text NOT NULL,
    order_id uuid NOT NULL, authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL,
    projection jsonb NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, order_id),
    UNIQUE (tenant_id, projection_name, generation, order_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_manufacturing_read.genealogy_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_tenant_id(),
    projection_name text NOT NULL DEFAULT 'manufacturing.orders' CHECK (projection_name = 'manufacturing.orders'),
    generation uuid NOT NULL, partition_key text NOT NULL,
    output_lot_or_serial text NOT NULL, source_lot_or_serial text NOT NULL, order_id uuid NOT NULL,
    result_fact_id uuid NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, output_lot_or_serial, source_lot_or_serial, result_fact_id),
    FOREIGN KEY (tenant_id, projection_name, generation, order_id)
      REFERENCES erpx_manufacturing_read.order_projection(tenant_id, projection_name, generation, order_id) ON DELETE CASCADE
);
CREATE TABLE erpx_manufacturing_read.wip_cost_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_tenant_id(),
    projection_name text NOT NULL DEFAULT 'manufacturing.orders' CHECK (projection_name = 'manufacturing.orders'),
    generation uuid NOT NULL, partition_key text NOT NULL, order_id uuid NOT NULL,
    wip_minor_units bigint NOT NULL, currency char(3) NOT NULL, reconciled boolean NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, order_id),
    FOREIGN KEY (tenant_id, projection_name, generation, order_id)
      REFERENCES erpx_manufacturing_read.order_projection(tenant_id, projection_name, generation, order_id) ON DELETE CASCADE
);
CREATE TABLE erpx_manufacturing_read.governed_record_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_tenant_id(), projection_name text NOT NULL DEFAULT 'manufacturing.orders' CHECK (projection_name='manufacturing.orders'),
    generation uuid NOT NULL,partition_key text NOT NULL,record_id uuid NOT NULL,record_kind text NOT NULL CHECK(record_kind IN('routing','batch','result')),
    authority_scope jsonb NOT NULL,lifecycle_state text NOT NULL,projection jsonb NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK(source_aggregate_version>0),source_position bigint NOT NULL CHECK(source_position>0),projected_at timestamptz NOT NULL,
    PRIMARY KEY(tenant_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY(tenant_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation) ON DELETE CASCADE
);

CREATE TABLE erpx_planning_read.run_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_planning_read.current_tenant_id(),
    projection_name text NOT NULL DEFAULT 'planning.runs' CHECK (projection_name = 'planning.runs'),
    generation uuid NOT NULL, partition_key text NOT NULL, run_id uuid NOT NULL, scenario_id uuid NOT NULL,
    authority_scope jsonb NOT NULL, input_digest text NOT NULL CHECK (input_digest ~ '^[0-9a-f]{64}$'),
    projection jsonb NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, run_id),
    UNIQUE (tenant_id, projection_name, generation, run_id),
    FOREIGN KEY (tenant_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(tenant_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_planning_read.pegging_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_planning_read.current_tenant_id(),
    projection_name text NOT NULL DEFAULT 'planning.runs' CHECK (projection_name = 'planning.runs'),
    generation uuid NOT NULL, partition_key text NOT NULL, run_id uuid NOT NULL,
    demand_id uuid NOT NULL, supply_id uuid NOT NULL, quantity_units bigint NOT NULL CHECK (quantity_units > 0),
    quantity_scale smallint NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, run_id, demand_id, supply_id),
    FOREIGN KEY (tenant_id, projection_name, generation, run_id)
      REFERENCES erpx_planning_read.run_projection(tenant_id, projection_name, generation, run_id) ON DELETE CASCADE
);
CREATE TABLE erpx_planning_read.exception_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_planning_read.current_tenant_id(),
    projection_name text NOT NULL DEFAULT 'planning.runs' CHECK (projection_name = 'planning.runs'),
    generation uuid NOT NULL, partition_key text NOT NULL, run_id uuid NOT NULL,
    exception_key text NOT NULL, exception_kind text NOT NULL CHECK (exception_kind IN ('shortage', 'capacity_overload')),
    payload jsonb NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    PRIMARY KEY (tenant_id, projection_name, generation, partition_key, run_id, exception_key),
    FOREIGN KEY (tenant_id, projection_name, generation, run_id)
      REFERENCES erpx_planning_read.run_projection(tenant_id, projection_name, generation, run_id) ON DELETE CASCADE
);
CREATE TABLE erpx_planning_read.plan_projection (
    tenant_id uuid NOT NULL DEFAULT erpx_planning_read.current_tenant_id(),projection_name text NOT NULL DEFAULT 'planning.runs' CHECK(projection_name='planning.runs'),generation uuid NOT NULL,partition_key text NOT NULL,
    plan_id uuid NOT NULL,plan_kind text NOT NULL CHECK(plan_kind IN('demand_plan','supply_plan','capacity_plan')),authority_scope jsonb NOT NULL,lifecycle_state text NOT NULL,projection jsonb NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK(source_aggregate_version>0),source_position bigint NOT NULL CHECK(source_position>0),projected_at timestamptz NOT NULL,
    PRIMARY KEY(tenant_id,projection_name,generation,partition_key,plan_id),FOREIGN KEY(tenant_id,projection_name,generation)REFERENCES erpx_cqrs_read.projection_generations(tenant_id,projection_name,generation)ON DELETE CASCADE
);

DO $$
DECLARE relation_name text;
BEGIN
  FOREACH relation_name IN ARRAY ARRAY[
    'erpx_manufacturing_read.order_projection', 'erpx_manufacturing_read.genealogy_projection',
    'erpx_manufacturing_read.wip_cost_projection','erpx_manufacturing_read.governed_record_projection', 'erpx_planning_read.run_projection',
    'erpx_planning_read.pegging_projection', 'erpx_planning_read.exception_projection','erpx_planning_read.plan_projection'
  ] LOOP
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('CREATE POLICY tenant_isolation ON %s USING (tenant_id = nullif(current_setting(''app.tenant_id'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''app.tenant_id'', true), '''')::uuid)', relation_name);
  END LOOP;
END $$;
