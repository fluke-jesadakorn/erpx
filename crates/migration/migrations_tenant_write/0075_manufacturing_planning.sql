CREATE SCHEMA IF NOT EXISTS erpx_manufacturing_write;
CREATE SCHEMA IF NOT EXISTS erpx_planning_write;

CREATE TABLE erpx_manufacturing_write.aggregate_versions (
    tenant_id uuid NOT NULL,
    aggregate_key text NOT NULL,
    aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('specification', 'order', 'routing', 'batch', 'result')),
    aggregate_id uuid NOT NULL,
    authority_scope jsonb NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant_id, aggregate_key),
    UNIQUE (tenant_id, aggregate_kind, aggregate_id)
);

CREATE TABLE erpx_manufacturing_write.specifications (
    tenant_id uuid NOT NULL, aggregate_key text NOT NULL, specification_id uuid NOT NULL,
    manufacturing_mode text NOT NULL CHECK (manufacturing_mode IN ('discrete','process')),
    output_item jsonb NOT NULL, output_quantity jsonb NOT NULL, material_lines jsonb NOT NULL,
    routing_id uuid NOT NULL, routing_steps jsonb NOT NULL,
    lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('draft','released','retired')),
    PRIMARY KEY (tenant_id,specification_id), UNIQUE (tenant_id,aggregate_key),
    FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_manufacturing_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);

CREATE TABLE erpx_manufacturing_write.production_orders (
    tenant_id uuid NOT NULL, aggregate_key text NOT NULL, order_id uuid NOT NULL,
    specification_id uuid NOT NULL, specification_version bigint NOT NULL CHECK (specification_version > 0),
    manufacturing_mode text NOT NULL CHECK (manufacturing_mode IN ('discrete','process')),
    planned_output jsonb NOT NULL, lifecycle_state text NOT NULL,
    material_balances jsonb NOT NULL, material_fact_ids jsonb NOT NULL, production_results jsonb NOT NULL,
    labor_results jsonb NOT NULL, quality_dispositions jsonb NOT NULL, cost_facts jsonb NOT NULL,
    correlation_id uuid NOT NULL,
    PRIMARY KEY (tenant_id,order_id), UNIQUE (tenant_id,aggregate_key),
    FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_manufacturing_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,specification_id) REFERENCES erpx_manufacturing_write.specifications(tenant_id,specification_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_manufacturing_write.governed_records (
    tenant_id uuid NOT NULL, aggregate_key text NOT NULL, record_id uuid NOT NULL,
    record_kind text NOT NULL CHECK (record_kind IN ('routing','batch','result')),
    parent_record jsonb NOT NULL, evidence_digest text NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('proposed','active','completed','cancelled')),
    PRIMARY KEY (tenant_id,record_id), UNIQUE (tenant_id,aggregate_key),
    FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_manufacturing_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);

CREATE TABLE erpx_manufacturing_write.material_fact_guards (
    tenant_id uuid NOT NULL,
    order_id uuid NOT NULL,
    fact_id uuid NOT NULL,
    material_line integer NOT NULL CHECK (material_line > 0),
    quantity_units bigint NOT NULL CHECK (quantity_units > 0),
    quantity_scale smallint NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    occurred_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id, fact_id)
);

CREATE TABLE erpx_manufacturing_write.genealogy_guards (
    tenant_id uuid NOT NULL,
    order_id uuid NOT NULL,
    result_fact_id uuid NOT NULL,
    output_lot_or_serial text NOT NULL CHECK (length(btrim(output_lot_or_serial)) BETWEEN 1 AND 128),
    source_lot_or_serial text NOT NULL CHECK (length(btrim(source_lot_or_serial)) BETWEEN 1 AND 128),
    PRIMARY KEY (tenant_id, result_fact_id, source_lot_or_serial)
);

CREATE TABLE erpx_manufacturing_write.command_receipts (
    tenant_id uuid NOT NULL,
    idempotency_key text NOT NULL,
    command_id uuid NOT NULL,
    request_digest text NOT NULL CHECK (request_digest ~ '^[0-9A-Fa-f]{64}$'),
    aggregate_keys text[] NOT NULL,
    aggregate_versions bigint[] NOT NULL,
    committed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant_id, idempotency_key),
    UNIQUE (tenant_id, command_id)
);

CREATE TABLE erpx_manufacturing_write.audit_log (
    tenant_id uuid NOT NULL, command_id uuid NOT NULL, actor_principal_id uuid NOT NULL,
    action text NOT NULL, authority_scope jsonb NOT NULL, aggregate_key text NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0), correlation_id uuid NOT NULL,
    evidence_digest text NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY (tenant_id,command_id)
);

CREATE TABLE erpx_manufacturing_write.outbox (
    position bigint GENERATED ALWAYS AS IDENTITY,
    tenant_id uuid NOT NULL,
    aggregate_key text NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
    event_type text NOT NULL,
    event_version integer NOT NULL DEFAULT 1 CHECK (event_version = 1),
    correlation_id uuid NOT NULL,
    payload jsonb NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    published_at timestamptz,
    PRIMARY KEY (tenant_id,position), UNIQUE (tenant_id,aggregate_key,aggregate_version,event_type)
);

CREATE VIEW erpx_manufacturing_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',s.specification_id,'version',v.aggregate_version,'scope',v.authority_scope,
  'mode',s.manufacturing_mode,'output_item',s.output_item,'output_quantity',s.output_quantity,
  'materials',s.material_lines,'routing_id',s.routing_id,'routing',s.routing_steps,
  'status',s.lifecycle_state,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_manufacturing_write.aggregate_versions v JOIN erpx_manufacturing_write.specifications s
 ON s.tenant_id=v.tenant_id AND s.aggregate_key=v.aggregate_key
UNION ALL
SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',o.order_id,'version',v.aggregate_version,'scope',v.authority_scope,
  'specification',jsonb_build_object('specification_id',o.specification_id,'version',o.specification_version),
  'mode',o.manufacturing_mode,'planned_output',o.planned_output,'status',o.lifecycle_state,
  'material',o.material_balances,'material_fact_ids',o.material_fact_ids,'results',o.production_results,
  'labor',o.labor_results,'dispositions',o.quality_dispositions,'costs',o.cost_facts,
  'correlation_id',o.correlation_id,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_manufacturing_write.aggregate_versions v JOIN erpx_manufacturing_write.production_orders o
 ON o.tenant_id=v.tenant_id AND o.aggregate_key=v.aggregate_key
UNION ALL
SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.record_id,'version',v.aggregate_version,'scope',v.authority_scope,
  'kind',r.record_kind,'parent',r.parent_record,'evidence_digest',r.evidence_digest,
  'status',r.lifecycle_state,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_manufacturing_write.aggregate_versions v JOIN erpx_manufacturing_write.governed_records r
 ON r.tenant_id=v.tenant_id AND r.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_manufacturing_write.persist_normalized(
 p_tenant_id uuid,p_aggregate_key text,p_kind text,p_id uuid,p_scope jsonb,p_version bigint,p_document jsonb
) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO erpx_manufacturing_write.aggregate_versions
  (tenant_id,aggregate_key,aggregate_kind,aggregate_id,authority_scope,aggregate_version,updated_at)
 VALUES (p_tenant_id,p_aggregate_key,p_kind,p_id,p_scope,p_version,(p_document->>'updated_at')::timestamptz)
 ON CONFLICT (tenant_id,aggregate_key) DO UPDATE SET authority_scope=EXCLUDED.authority_scope,
  aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_manufacturing_write.aggregate_versions.aggregate_version < EXCLUDED.aggregate_version;
 IF p_kind='specification' THEN
  INSERT INTO erpx_manufacturing_write.specifications
   (tenant_id,aggregate_key,specification_id,manufacturing_mode,output_item,output_quantity,material_lines,routing_id,routing_steps,lifecycle_state)
  VALUES (p_tenant_id,p_aggregate_key,p_id,p_document->>'mode',p_document->'output_item',p_document->'output_quantity',
   p_document->'materials',(p_document->>'routing_id')::uuid,p_document->'routing',p_document->>'status')
  ON CONFLICT (tenant_id,specification_id) DO UPDATE SET material_lines=EXCLUDED.material_lines,
   routing_steps=EXCLUDED.routing_steps,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF p_kind='order' THEN
  INSERT INTO erpx_manufacturing_write.production_orders
   (tenant_id,aggregate_key,order_id,specification_id,specification_version,manufacturing_mode,planned_output,lifecycle_state,
    material_balances,material_fact_ids,production_results,labor_results,quality_dispositions,cost_facts,correlation_id)
  VALUES (p_tenant_id,p_aggregate_key,p_id,(p_document->'specification'->>'specification_id')::uuid,
   (p_document->'specification'->>'version')::bigint,p_document->>'mode',p_document->'planned_output',p_document->>'status',
   p_document->'material',p_document->'material_fact_ids',p_document->'results',p_document->'labor',
   p_document->'dispositions',p_document->'costs',(p_document->>'correlation_id')::uuid)
  ON CONFLICT (tenant_id,order_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,
   material_balances=EXCLUDED.material_balances,material_fact_ids=EXCLUDED.material_fact_ids,
   production_results=EXCLUDED.production_results,labor_results=EXCLUDED.labor_results,
   quality_dispositions=EXCLUDED.quality_dispositions,cost_facts=EXCLUDED.cost_facts;
 ELSIF p_kind IN ('routing','batch','result') THEN
  INSERT INTO erpx_manufacturing_write.governed_records
   (tenant_id,aggregate_key,record_id,record_kind,parent_record,evidence_digest,lifecycle_state)
  VALUES (p_tenant_id,p_aggregate_key,p_id,p_kind,p_document->'parent',p_document->>'evidence_digest',p_document->>'status')
  ON CONFLICT (tenant_id,record_id) DO UPDATE SET parent_record=EXCLUDED.parent_record,
   evidence_digest=EXCLUDED.evidence_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported manufacturing aggregate kind'; END IF;
END $$;

CREATE TABLE erpx_planning_write.aggregate_versions (
    tenant_id uuid NOT NULL,
    aggregate_key text NOT NULL,
    aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('scenario', 'run', 'recommendation', 'demand_plan', 'supply_plan', 'capacity_plan')),
    aggregate_id uuid NOT NULL,
    authority_scope jsonb NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant_id, aggregate_key),
    UNIQUE (tenant_id, aggregate_kind, aggregate_id)
);

CREATE TABLE erpx_planning_write.scenarios (
    tenant_id uuid NOT NULL,aggregate_key text NOT NULL,scenario_id uuid NOT NULL,planning_modes jsonb NOT NULL,
    input_position bigint NOT NULL CHECK (input_position > 0),input_refs jsonb NOT NULL,policy jsonb NOT NULL,
    demand_facts jsonb NOT NULL,supply_facts jsonb NOT NULL,capacity_buckets jsonb NOT NULL,capacity_requirements jsonb NOT NULL,
    lifecycle_state text NOT NULL,last_failure_reason text,
    PRIMARY KEY (tenant_id,scenario_id),UNIQUE (tenant_id,aggregate_key),
    FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_planning_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_planning_write.runs (
    tenant_id uuid NOT NULL,aggregate_key text NOT NULL,run_id uuid NOT NULL,scenario_id uuid NOT NULL,
    scenario_version bigint NOT NULL CHECK (scenario_version > 0),policy_version bigint NOT NULL CHECK (policy_version > 0),
    input_position bigint NOT NULL CHECK (input_position > 0),input_digest text NOT NULL CHECK (input_digest ~ '^[0-9a-f]{64}$'),
    shortages jsonb NOT NULL,pegging jsonb NOT NULL,overloads jsonb NOT NULL,recommendations jsonb NOT NULL,completed_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id,run_id),UNIQUE (tenant_id,aggregate_key),
    FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_planning_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id,scenario_id) REFERENCES erpx_planning_write.scenarios(tenant_id,scenario_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_planning_write.recommendations (
    tenant_id uuid NOT NULL,aggregate_key text NOT NULL,recommendation_id uuid NOT NULL,recommendation_kind text NOT NULL,
    item_ref jsonb NOT NULL,destination_ref jsonb NOT NULL,source_ref jsonb,quantity jsonb NOT NULL,required_on date NOT NULL,
    demand_id uuid NOT NULL,lifecycle_state text NOT NULL,override_evidence jsonb NOT NULL,release_id uuid,
    PRIMARY KEY (tenant_id,recommendation_id),UNIQUE (tenant_id,aggregate_key),
    FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_planning_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.plans (
    tenant_id uuid NOT NULL,aggregate_key text NOT NULL,plan_id uuid NOT NULL,
    plan_kind text NOT NULL CHECK (plan_kind IN ('demand_plan','supply_plan','capacity_plan')),
    scenario_ref jsonb NOT NULL,input_position bigint NOT NULL CHECK (input_position > 0),
    policy_digest text NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('draft','frozen','published','superseded','cancelled')),
    PRIMARY KEY (tenant_id,plan_id),UNIQUE (tenant_id,aggregate_key),
    FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_planning_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.input_pins (
    tenant_id uuid NOT NULL,
    scenario_id uuid NOT NULL,
    input_record_id uuid NOT NULL,
    input_version bigint NOT NULL CHECK (input_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    policy_id uuid NOT NULL,
    policy_version bigint NOT NULL CHECK (policy_version > 0),
    PRIMARY KEY (tenant_id, scenario_id, input_record_id),
    FOREIGN KEY (tenant_id,scenario_id) REFERENCES erpx_planning_write.scenarios(tenant_id,scenario_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.run_guards (
    tenant_id uuid NOT NULL,
    run_id uuid NOT NULL,
    scenario_id uuid NOT NULL,
    scenario_version bigint NOT NULL CHECK (scenario_version > 0),
    policy_version bigint NOT NULL CHECK (policy_version > 0),
    input_position bigint NOT NULL CHECK (input_position > 0),
    input_digest text NOT NULL CHECK (input_digest ~ '^[0-9a-f]{64}$'),
    completed_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id, run_id),
    UNIQUE (tenant_id, scenario_id, scenario_version, input_digest),
    FOREIGN KEY (tenant_id,run_id) REFERENCES erpx_planning_write.runs(tenant_id,run_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.release_guards (
    tenant_id uuid NOT NULL,
    release_id uuid NOT NULL,
    recommendation_id uuid NOT NULL,
    recommendation_version bigint NOT NULL CHECK (recommendation_version > 0),
    owner_correlation_id uuid NOT NULL,
    accepted boolean NOT NULL,
    rejection_reason text,
    decided_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant_id, release_id),
    UNIQUE (tenant_id, recommendation_id, recommendation_version),
    FOREIGN KEY (tenant_id,recommendation_id) REFERENCES erpx_planning_write.recommendations(tenant_id,recommendation_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.command_receipts (
    tenant_id uuid NOT NULL,
    idempotency_key text NOT NULL,
    command_id uuid NOT NULL,
    request_digest text NOT NULL CHECK (request_digest ~ '^[0-9A-Fa-f]{64}$'),
    aggregate_keys text[] NOT NULL,
    aggregate_versions bigint[] NOT NULL,
    committed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant_id, idempotency_key),
    UNIQUE (tenant_id, command_id)
);

CREATE TABLE erpx_planning_write.audit_log (
    tenant_id uuid NOT NULL,command_id uuid NOT NULL,actor_principal_id uuid NOT NULL,
    action text NOT NULL,authority_scope jsonb NOT NULL,aggregate_keys text[] NOT NULL,aggregate_versions bigint[] NOT NULL,
    correlation_id uuid NOT NULL,evidence_digest text NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY (tenant_id,command_id)
);

CREATE TABLE erpx_planning_write.outbox (
    position bigint GENERATED ALWAYS AS IDENTITY,
    tenant_id uuid NOT NULL,
    aggregate_key text NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
    event_type text NOT NULL,
    event_version integer NOT NULL DEFAULT 1 CHECK (event_version = 1),
    correlation_id uuid NOT NULL,
    payload jsonb NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    published_at timestamptz,
    PRIMARY KEY (tenant_id,position),UNIQUE (tenant_id,aggregate_key,aggregate_version,event_type)
);

CREATE VIEW erpx_planning_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',s.scenario_id,'version',v.aggregate_version,'scope',v.authority_scope,'modes',s.planning_modes,
  'input_position',s.input_position,'inputs',s.input_refs,'policy',s.policy,'demands',s.demand_facts,
  'supplies',s.supply_facts,'capacity',s.capacity_buckets,'capacity_requirements',s.capacity_requirements,
  'status',s.lifecycle_state,'last_failure_reason',s.last_failure_reason,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_planning_write.aggregate_versions v JOIN erpx_planning_write.scenarios s
 ON s.tenant_id=v.tenant_id AND s.aggregate_key=v.aggregate_key
UNION ALL SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.run_id,'scenario_id',r.scenario_id,'scenario_version',r.scenario_version,
  'policy_version',r.policy_version,'input_position',r.input_position,'input_digest',r.input_digest,
  'shortages',r.shortages,'pegging',r.pegging,'overloads',r.overloads,'recommendations',r.recommendations,'completed_at',r.completed_at)
FROM erpx_planning_write.aggregate_versions v JOIN erpx_planning_write.runs r ON r.tenant_id=v.tenant_id AND r.aggregate_key=v.aggregate_key
UNION ALL SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.recommendation_id,'version',v.aggregate_version,'kind',r.recommendation_kind,'item',r.item_ref,
  'destination',r.destination_ref,'source',r.source_ref,'quantity',r.quantity,'required_on',r.required_on,
  'demand_id',r.demand_id,'status',r.lifecycle_state,'overrides',r.override_evidence,'release_id',r.release_id)
FROM erpx_planning_write.aggregate_versions v JOIN erpx_planning_write.recommendations r
 ON r.tenant_id=v.tenant_id AND r.aggregate_key=v.aggregate_key
UNION ALL SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',p.plan_id,'version',v.aggregate_version,'scope',v.authority_scope,'kind',p.plan_kind,
  'scenario',p.scenario_ref,'input_position',p.input_position,'policy_digest',p.policy_digest,
  'status',p.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_planning_write.aggregate_versions v JOIN erpx_planning_write.plans p
 ON p.tenant_id=v.tenant_id AND p.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_planning_write.persist_normalized(
 p_tenant_id uuid,p_aggregate_key text,p_kind text,p_id uuid,p_scope jsonb,p_version bigint,p_document jsonb
) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO erpx_planning_write.aggregate_versions
  (tenant_id,aggregate_key,aggregate_kind,aggregate_id,authority_scope,aggregate_version,updated_at)
 VALUES (p_tenant_id,p_aggregate_key,p_kind,p_id,p_scope,p_version,
  COALESCE((p_document->>'updated_at')::timestamptz,(p_document->>'completed_at')::timestamptz,clock_timestamp()))
 ON CONFLICT (tenant_id,aggregate_key) DO UPDATE SET authority_scope=EXCLUDED.authority_scope,
  aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_planning_write.aggregate_versions.aggregate_version < EXCLUDED.aggregate_version;
 IF p_kind='scenario' THEN
  INSERT INTO erpx_planning_write.scenarios
   (tenant_id,aggregate_key,scenario_id,planning_modes,input_position,input_refs,policy,demand_facts,supply_facts,capacity_buckets,capacity_requirements,lifecycle_state,last_failure_reason)
  VALUES (p_tenant_id,p_aggregate_key,p_id,p_document->'modes',(p_document->>'input_position')::bigint,
   p_document->'inputs',p_document->'policy',p_document->'demands',p_document->'supplies',p_document->'capacity',
   p_document->'capacity_requirements',p_document->>'status',p_document->>'last_failure_reason')
  ON CONFLICT (tenant_id,scenario_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,last_failure_reason=EXCLUDED.last_failure_reason;
 ELSIF p_kind='run' THEN
  INSERT INTO erpx_planning_write.runs
   (tenant_id,aggregate_key,run_id,scenario_id,scenario_version,policy_version,input_position,input_digest,shortages,pegging,overloads,recommendations,completed_at)
  VALUES (p_tenant_id,p_aggregate_key,p_id,(p_document->>'scenario_id')::uuid,(p_document->>'scenario_version')::bigint,
   (p_document->>'policy_version')::bigint,(p_document->>'input_position')::bigint,p_document->>'input_digest',
   p_document->'shortages',p_document->'pegging',p_document->'overloads',p_document->'recommendations',(p_document->>'completed_at')::timestamptz)
  ON CONFLICT (tenant_id,run_id) DO NOTHING;
  INSERT INTO erpx_planning_write.run_guards
   (tenant_id,run_id,scenario_id,scenario_version,policy_version,input_position,input_digest,completed_at)
  VALUES (p_tenant_id,p_id,(p_document->>'scenario_id')::uuid,(p_document->>'scenario_version')::bigint,
   (p_document->>'policy_version')::bigint,(p_document->>'input_position')::bigint,p_document->>'input_digest',
   (p_document->>'completed_at')::timestamptz) ON CONFLICT (tenant_id,run_id) DO NOTHING;
 ELSIF p_kind='recommendation' THEN
  INSERT INTO erpx_planning_write.recommendations
   (tenant_id,aggregate_key,recommendation_id,recommendation_kind,item_ref,destination_ref,source_ref,quantity,required_on,demand_id,lifecycle_state,override_evidence,release_id)
  VALUES (p_tenant_id,p_aggregate_key,p_id,p_document->>'kind',p_document->'item',p_document->'destination',p_document->'source',
   p_document->'quantity',(p_document->>'required_on')::date,(p_document->>'demand_id')::uuid,p_document->>'status',
   p_document->'overrides',(p_document->>'release_id')::uuid)
  ON CONFLICT (tenant_id,recommendation_id) DO UPDATE SET quantity=EXCLUDED.quantity,
   lifecycle_state=EXCLUDED.lifecycle_state,override_evidence=EXCLUDED.override_evidence,release_id=EXCLUDED.release_id;
 ELSIF p_kind IN ('demand_plan','supply_plan','capacity_plan') THEN
  INSERT INTO erpx_planning_write.plans
   (tenant_id,aggregate_key,plan_id,plan_kind,scenario_ref,input_position,policy_digest,lifecycle_state)
  VALUES (p_tenant_id,p_aggregate_key,p_id,p_kind,p_document->'scenario',(p_document->>'input_position')::bigint,
   p_document->>'policy_digest',p_document->>'status')
  ON CONFLICT (tenant_id,plan_id) DO UPDATE SET scenario_ref=EXCLUDED.scenario_ref,
   input_position=EXCLUDED.input_position,policy_digest=EXCLUDED.policy_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported planning aggregate kind'; END IF;
END $$;

DO $$
DECLARE relation_name text;
BEGIN
  FOREACH relation_name IN ARRAY ARRAY[
    'erpx_manufacturing_write.aggregate_versions','erpx_manufacturing_write.specifications','erpx_manufacturing_write.production_orders','erpx_manufacturing_write.governed_records',
    'erpx_manufacturing_write.material_fact_guards','erpx_manufacturing_write.genealogy_guards','erpx_manufacturing_write.command_receipts',
    'erpx_manufacturing_write.audit_log','erpx_manufacturing_write.outbox','erpx_planning_write.aggregate_versions',
    'erpx_planning_write.scenarios','erpx_planning_write.runs','erpx_planning_write.recommendations','erpx_planning_write.plans',
    'erpx_planning_write.input_pins', 'erpx_planning_write.run_guards',
    'erpx_planning_write.release_guards','erpx_planning_write.command_receipts','erpx_planning_write.audit_log','erpx_planning_write.outbox'
  ] LOOP
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', relation_name);
    EXECUTE format(
      'CREATE POLICY tenant_isolation ON %s USING (tenant_id = nullif(current_setting(''app.tenant_id'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''app.tenant_id'', true), '''')::uuid)',
      relation_name
    );
  END LOOP;
END $$;
