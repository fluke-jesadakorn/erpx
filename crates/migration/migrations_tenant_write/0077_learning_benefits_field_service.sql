CREATE SCHEMA IF NOT EXISTS erpx_learning_write;
CREATE SCHEMA IF NOT EXISTS erpx_benefits_write;
CREATE SCHEMA IF NOT EXISTS erpx_field_service_write;

CREATE TABLE erpx_learning_write.aggregate_versions (
  tenant_id uuid NOT NULL, aggregate_key text NOT NULL, aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('course','enrollment','assignment','completion')),
  aggregate_id uuid NOT NULL, authority_scope jsonb NOT NULL, aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
  updated_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY (tenant_id, aggregate_key), UNIQUE (tenant_id, aggregate_kind, aggregate_id)
);
CREATE TABLE erpx_learning_write.courses (
  tenant_id uuid NOT NULL,aggregate_key text NOT NULL,course_id uuid NOT NULL,title text NOT NULL,
  passing_basis_points integer NOT NULL CHECK (passing_basis_points BETWEEN 0 AND 10000),
  required_attendance_minutes integer NOT NULL CHECK (required_attendance_minutes > 0),provider_ref jsonb,
  lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('draft','published','retired')),
  PRIMARY KEY (tenant_id,course_id),UNIQUE (tenant_id,aggregate_key),
  FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_learning_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.enrollments (
  tenant_id uuid NOT NULL,aggregate_key text NOT NULL,enrollment_id uuid NOT NULL,assignment_id uuid NOT NULL,
  learner_ref jsonb NOT NULL,learner_principal_id uuid NOT NULL,course_id uuid NOT NULL,course_version bigint NOT NULL,
  due_on date NOT NULL,lifecycle_state text NOT NULL,attendance_facts jsonb NOT NULL,provider_results jsonb NOT NULL,
  waiver_evidence jsonb,completion_evidence jsonb,
  PRIMARY KEY (tenant_id,enrollment_id),UNIQUE (tenant_id,aggregate_key),
  FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_learning_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id,course_id) REFERENCES erpx_learning_write.courses(tenant_id,course_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.governed_records (
  tenant_id uuid NOT NULL,aggregate_key text NOT NULL,record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK(record_kind IN ('assignment','completion')),subject_ref jsonb NOT NULL,
  evidence_digest text NOT NULL CHECK(evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
  lifecycle_state text NOT NULL CHECK(lifecycle_state IN ('proposed','assigned','in_progress','completed','waived','cancelled')),
  PRIMARY KEY(tenant_id,record_id),UNIQUE(tenant_id,aggregate_key),
  FOREIGN KEY(tenant_id,aggregate_key) REFERENCES erpx_learning_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.attendance_guards (
  tenant_id uuid NOT NULL, enrollment_id uuid NOT NULL, attendance_fact_id uuid NOT NULL, learner_principal_id uuid NOT NULL,
  recorded_by_principal_id uuid NOT NULL CHECK (learner_principal_id <> recorded_by_principal_id), evidence_record_id uuid NOT NULL,
  evidence_version bigint NOT NULL CHECK (evidence_version > 0), attended_minutes integer NOT NULL CHECK (attended_minutes > 0), PRIMARY KEY (tenant_id, attendance_fact_id),
  FOREIGN KEY (tenant_id,enrollment_id) REFERENCES erpx_learning_write.enrollments(tenant_id,enrollment_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.provider_result_guards (
  tenant_id uuid NOT NULL, enrollment_id uuid NOT NULL, result_id uuid NOT NULL, provider_attempt_id text NOT NULL,
  evidence_record_id uuid NOT NULL, evidence_version bigint NOT NULL CHECK (evidence_version > 0), score_basis_points integer NOT NULL CHECK (score_basis_points BETWEEN 0 AND 10000), passed boolean NOT NULL,
  PRIMARY KEY (tenant_id, result_id), UNIQUE (tenant_id, provider_attempt_id),
  FOREIGN KEY (tenant_id,enrollment_id) REFERENCES erpx_learning_write.enrollments(tenant_id,enrollment_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.completion_facts (
  tenant_id uuid NOT NULL, enrollment_id uuid NOT NULL, enrollment_version bigint NOT NULL CHECK (enrollment_version > 0),
  employee_record_id uuid NOT NULL, course_id uuid NOT NULL, course_version bigint NOT NULL CHECK (course_version > 0), completed_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id, enrollment_id, enrollment_version),
  FOREIGN KEY (tenant_id,enrollment_id) REFERENCES erpx_learning_write.enrollments(tenant_id,enrollment_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_benefits_write.aggregate_versions (
  tenant_id uuid NOT NULL, aggregate_key text NOT NULL, aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('plan','enrollment','window','handoff')),
  aggregate_id uuid NOT NULL, authority_scope jsonb NOT NULL, aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
  updated_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY (tenant_id, aggregate_key), UNIQUE (tenant_id, aggregate_kind, aggregate_id)
);
CREATE TABLE erpx_benefits_write.plans (
  tenant_id uuid NOT NULL,aggregate_key text NOT NULL,plan_id uuid NOT NULL,plan_name text NOT NULL,currency char(3) NOT NULL,
  eligibility_class text NOT NULL,minimum_tenure_days integer NOT NULL,contribution_rule jsonb NOT NULL,
  lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('draft','published','retired')),
  PRIMARY KEY (tenant_id,plan_id),UNIQUE (tenant_id,aggregate_key),
  FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_benefits_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.enrollments (
  tenant_id uuid NOT NULL,aggregate_key text NOT NULL,enrollment_id uuid NOT NULL,plan_id uuid NOT NULL,plan_version bigint NOT NULL,
  window_id uuid NOT NULL,employee_ref jsonb NOT NULL,eligibility_decision jsonb NOT NULL,life_event jsonb,
  dependents jsonb NOT NULL,election jsonb,provider_acknowledgements jsonb NOT NULL,payroll_handoff_id uuid,
  lifecycle_state text NOT NULL,
  PRIMARY KEY (tenant_id,enrollment_id),UNIQUE (tenant_id,aggregate_key),
  FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_benefits_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id,plan_id) REFERENCES erpx_benefits_write.plans(tenant_id,plan_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.governed_records (
  tenant_id uuid NOT NULL,aggregate_key text NOT NULL,record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK(record_kind IN ('window','handoff')),subject_ref jsonb NOT NULL,
  evidence_digest text NOT NULL CHECK(evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
  lifecycle_state text NOT NULL CHECK(lifecycle_state IN ('draft','open','closed','submitted','accepted','rejected','cancelled')),
  PRIMARY KEY(tenant_id,record_id),UNIQUE(tenant_id,aggregate_key),
  FOREIGN KEY(tenant_id,aggregate_key) REFERENCES erpx_benefits_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.eligibility_guards (
  tenant_id uuid NOT NULL, enrollment_id uuid NOT NULL, employee_record_id uuid NOT NULL, employee_version bigint NOT NULL CHECK (employee_version > 0),
  source_record_id uuid NOT NULL, source_version bigint NOT NULL CHECK (source_version > 0), eligible boolean NOT NULL, evaluated_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id, enrollment_id),
  FOREIGN KEY (tenant_id,enrollment_id) REFERENCES erpx_benefits_write.enrollments(tenant_id,enrollment_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.provider_acknowledgement_guards (
  tenant_id uuid NOT NULL, enrollment_id uuid NOT NULL, acknowledgement_id uuid NOT NULL, request_key text NOT NULL,
  accepted boolean NOT NULL, provider_reference text NOT NULL, rejection_reason text, received_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id, acknowledgement_id), UNIQUE (tenant_id, request_key),
  FOREIGN KEY (tenant_id,enrollment_id) REFERENCES erpx_benefits_write.enrollments(tenant_id,enrollment_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.payroll_handoff_guards (
  tenant_id uuid NOT NULL, handoff_id uuid NOT NULL, enrollment_id uuid NOT NULL, enrollment_version bigint NOT NULL CHECK (enrollment_version > 0),
  owner_correlation_id uuid NOT NULL, accepted boolean NOT NULL, rejection_reason text, decided_at timestamptz NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (tenant_id, handoff_id), UNIQUE (tenant_id, enrollment_id, enrollment_version),
  FOREIGN KEY (tenant_id,enrollment_id) REFERENCES erpx_benefits_write.enrollments(tenant_id,enrollment_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_field_service_write.aggregate_versions (
  tenant_id uuid NOT NULL, aggregate_key text NOT NULL, aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('agreement','order','request','dispatch','visit','handoff')),
  aggregate_id uuid NOT NULL, authority_scope jsonb NOT NULL, aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
  updated_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY (tenant_id, aggregate_key), UNIQUE (tenant_id, aggregate_kind, aggregate_id)
);
CREATE TABLE erpx_field_service_write.agreements (
  tenant_id uuid NOT NULL,aggregate_key text NOT NULL,agreement_id uuid NOT NULL,customer_ref jsonb NOT NULL,
  covered_assets jsonb NOT NULL,covered_locations jsonb NOT NULL,service_codes jsonb NOT NULL,
  valid_from date NOT NULL,valid_through date NOT NULL,active boolean NOT NULL,
  PRIMARY KEY (tenant_id,agreement_id),UNIQUE (tenant_id,aggregate_key),
  FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_field_service_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.service_orders (
  tenant_id uuid NOT NULL,aggregate_key text NOT NULL,service_order_id uuid NOT NULL,request_id uuid NOT NULL,request_version bigint NOT NULL,
  agreement_id uuid NOT NULL,agreement_version bigint NOT NULL,customer_ref jsonb NOT NULL,asset_ref jsonb NOT NULL,location_ref jsonb NOT NULL,
  service_code text NOT NULL,required_skills jsonb NOT NULL,scheduled_on date,technician_qualification jsonb,dispatch_id uuid,visit_id uuid,
  part_reservations jsonb NOT NULL,part_fact_ids jsonb NOT NULL,visit_evidence jsonb NOT NULL,customer_signature jsonb,
  exceptions jsonb NOT NULL,billing_handoff_id uuid,lifecycle_state text NOT NULL,
  PRIMARY KEY (tenant_id,service_order_id),UNIQUE (tenant_id,aggregate_key),
  FOREIGN KEY (tenant_id,aggregate_key) REFERENCES erpx_field_service_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id,agreement_id) REFERENCES erpx_field_service_write.agreements(tenant_id,agreement_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.governed_records (
  tenant_id uuid NOT NULL,aggregate_key text NOT NULL,record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK(record_kind IN ('request','dispatch','visit','handoff')),subject_ref jsonb NOT NULL,
  evidence_digest text NOT NULL CHECK(evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
  lifecycle_state text NOT NULL CHECK(lifecycle_state IN ('proposed','accepted','scheduled','in_progress','completed','reconciled','rejected','cancelled')),
  PRIMARY KEY(tenant_id,record_id),UNIQUE(tenant_id,aggregate_key),
  FOREIGN KEY(tenant_id,aggregate_key) REFERENCES erpx_field_service_write.aggregate_versions(tenant_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.part_issue_guards (
  tenant_id uuid NOT NULL, service_order_id uuid NOT NULL, part_fact_id uuid NOT NULL, part_record_id uuid NOT NULL,
  quantity_units bigint NOT NULL CHECK (quantity_units > 0), quantity_scale smallint NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9), occurred_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id, part_fact_id),
  FOREIGN KEY (tenant_id,service_order_id) REFERENCES erpx_field_service_write.service_orders(tenant_id,service_order_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.visit_evidence_guards (
  tenant_id uuid NOT NULL, service_order_id uuid NOT NULL, evidence_record_id uuid NOT NULL, evidence_version bigint NOT NULL CHECK (evidence_version > 0),
  evidence_kind text NOT NULL CHECK (evidence_kind IN ('visit','customer_signature','signature_refusal','mismatch_exception')), PRIMARY KEY (tenant_id, service_order_id, evidence_record_id, evidence_version),
  FOREIGN KEY (tenant_id,service_order_id) REFERENCES erpx_field_service_write.service_orders(tenant_id,service_order_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.exception_guards (
  tenant_id uuid NOT NULL, exception_id uuid NOT NULL, service_order_id uuid NOT NULL,
  exception_kind text NOT NULL CHECK (exception_kind IN ('customer_mismatch','asset_mismatch','signature_refused','coverage_mismatch','inventory_mismatch')),
  reason text NOT NULL CHECK (length(btrim(reason)) BETWEEN 8 AND 500), evidence_record_id uuid NOT NULL, evidence_version bigint NOT NULL CHECK (evidence_version > 0),
  PRIMARY KEY (tenant_id, exception_id),
  FOREIGN KEY (tenant_id,service_order_id) REFERENCES erpx_field_service_write.service_orders(tenant_id,service_order_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.billing_handoff_guards (
  tenant_id uuid NOT NULL, handoff_id uuid NOT NULL, service_order_id uuid NOT NULL, service_order_version bigint NOT NULL CHECK (service_order_version > 0),
  owner_correlation_id uuid NOT NULL, accepted boolean NOT NULL, rejection_reason text, decided_at timestamptz NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (tenant_id, handoff_id), UNIQUE (tenant_id, service_order_id, service_order_version),
  FOREIGN KEY (tenant_id,service_order_id) REFERENCES erpx_field_service_write.service_orders(tenant_id,service_order_id) ON DELETE RESTRICT
);

DO $$
DECLARE schema_name text;
BEGIN
  FOREACH schema_name IN ARRAY ARRAY['erpx_learning_write','erpx_benefits_write','erpx_field_service_write'] LOOP
    EXECUTE format('CREATE TABLE %I.command_receipts (tenant_id uuid NOT NULL,idempotency_key text NOT NULL,command_id uuid NOT NULL,request_digest text NOT NULL CHECK (request_digest ~ ''^[0-9A-Fa-f]{64}$''),aggregate_key text NOT NULL,aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),committed_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY (tenant_id,idempotency_key),UNIQUE (tenant_id,command_id))', schema_name);
    EXECUTE format('CREATE TABLE %I.audit_log (tenant_id uuid NOT NULL,command_id uuid NOT NULL,actor_principal_id uuid NOT NULL,action text NOT NULL,authority_scope jsonb NOT NULL,aggregate_key text NOT NULL,aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),correlation_id uuid NOT NULL,evidence_digest text NOT NULL CHECK (evidence_digest ~ ''^[0-9A-Fa-f]{64}$''),occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY (tenant_id,command_id))', schema_name);
    EXECUTE format('CREATE TABLE %I.outbox (position bigint GENERATED ALWAYS AS IDENTITY,tenant_id uuid NOT NULL,aggregate_key text NOT NULL,aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),event_type text NOT NULL,event_version integer NOT NULL DEFAULT 1 CHECK (event_version = 1),correlation_id uuid NOT NULL,payload jsonb NOT NULL,occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),published_at timestamptz,PRIMARY KEY (tenant_id,position),UNIQUE (tenant_id,aggregate_key,aggregate_version,event_type))', schema_name);
  END LOOP;
END $$;

CREATE VIEW erpx_learning_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',c.course_id,'version',v.aggregate_version,'scope',v.authority_scope,'title',c.title,
  'passing_basis_points',c.passing_basis_points,'required_attendance_minutes',c.required_attendance_minutes,
  'provider',c.provider_ref,'status',c.lifecycle_state,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_learning_write.aggregate_versions v JOIN erpx_learning_write.courses c ON c.tenant_id=v.tenant_id AND c.aggregate_key=v.aggregate_key
UNION ALL SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',e.enrollment_id,'version',v.aggregate_version,'scope',v.authority_scope,'assignment_id',e.assignment_id,
  'learner',e.learner_ref,'learner_principal',e.learner_principal_id,
  'course',jsonb_build_object('course_id',e.course_id,'version',e.course_version),'due_on',e.due_on,'status',e.lifecycle_state,
  'attendance',e.attendance_facts,'provider_results',e.provider_results,'waiver',e.waiver_evidence,
  'completion',e.completion_evidence,'updated_at',v.updated_at)
FROM erpx_learning_write.aggregate_versions v JOIN erpx_learning_write.enrollments e ON e.tenant_id=v.tenant_id AND e.aggregate_key=v.aggregate_key
UNION ALL SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.record_id,'version',v.aggregate_version,'scope',v.authority_scope,'kind',r.record_kind,
 'subject',r.subject_ref,'evidence_digest',r.evidence_digest,'status',r.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_learning_write.aggregate_versions v JOIN erpx_learning_write.governed_records r ON r.tenant_id=v.tenant_id AND r.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_learning_write.persist_normalized(uuid,text,text,uuid,jsonb,bigint,jsonb) RETURNS void LANGUAGE plpgsql AS $$
DECLARE t alias for $1; k alias for $2; kind alias for $3; rid alias for $4; scope alias for $5; ver alias for $6; d alias for $7;
BEGIN
 INSERT INTO erpx_learning_write.aggregate_versions VALUES(t,k,kind,rid,scope,ver,(d->>'updated_at')::timestamptz)
 ON CONFLICT (tenant_id,aggregate_key) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_learning_write.aggregate_versions.aggregate_version<EXCLUDED.aggregate_version;
 IF kind='course' THEN
  INSERT INTO erpx_learning_write.courses VALUES(t,k,rid,d->>'title',(d->>'passing_basis_points')::integer,(d->>'required_attendance_minutes')::integer,d->'provider',d->>'status')
  ON CONFLICT (tenant_id,course_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF kind='enrollment' THEN
  INSERT INTO erpx_learning_write.enrollments VALUES(t,k,rid,(d->>'assignment_id')::uuid,d->'learner',(d->>'learner_principal')::uuid,
   (d->'course'->>'course_id')::uuid,(d->'course'->>'version')::bigint,(d->>'due_on')::date,d->>'status',
   d->'attendance',d->'provider_results',d->'waiver',d->'completion')
  ON CONFLICT (tenant_id,enrollment_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,attendance_facts=EXCLUDED.attendance_facts,
   provider_results=EXCLUDED.provider_results,waiver_evidence=EXCLUDED.waiver_evidence,completion_evidence=EXCLUDED.completion_evidence;
 ELSIF kind IN ('assignment','completion') THEN
  INSERT INTO erpx_learning_write.governed_records VALUES(t,k,rid,kind,d->'subject',d->>'evidence_digest',d->>'status')
  ON CONFLICT(tenant_id,record_id) DO UPDATE SET evidence_digest=EXCLUDED.evidence_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported learning aggregate kind'; END IF;
END $$;

CREATE VIEW erpx_benefits_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',p.plan_id,'version',v.aggregate_version,'scope',v.authority_scope,'name',p.plan_name,'currency',p.currency,
  'eligibility_class',p.eligibility_class,'minimum_tenure_days',p.minimum_tenure_days,'contribution',p.contribution_rule,
  'status',p.lifecycle_state,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_benefits_write.aggregate_versions v JOIN erpx_benefits_write.plans p ON p.tenant_id=v.tenant_id AND p.aggregate_key=v.aggregate_key
UNION ALL SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',e.enrollment_id,'version',v.aggregate_version,'scope',v.authority_scope,
  'plan',jsonb_build_object('plan_id',e.plan_id,'version',e.plan_version),'window',e.window_id,'employee',e.employee_ref,
  'eligibility',e.eligibility_decision,'life_event',e.life_event,'dependents',e.dependents,'election',e.election,
  'acknowledgements',e.provider_acknowledgements,'payroll_handoff_id',e.payroll_handoff_id,
  'status',e.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_benefits_write.aggregate_versions v JOIN erpx_benefits_write.enrollments e ON e.tenant_id=v.tenant_id AND e.aggregate_key=v.aggregate_key
UNION ALL SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.record_id,'version',v.aggregate_version,'scope',v.authority_scope,'kind',r.record_kind,
 'subject',r.subject_ref,'evidence_digest',r.evidence_digest,'status',r.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_benefits_write.aggregate_versions v JOIN erpx_benefits_write.governed_records r ON r.tenant_id=v.tenant_id AND r.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_benefits_write.persist_normalized(uuid,text,text,uuid,jsonb,bigint,jsonb) RETURNS void LANGUAGE plpgsql AS $$
DECLARE t alias for $1; k alias for $2; kind alias for $3; rid alias for $4; scope alias for $5; ver alias for $6; d alias for $7;
BEGIN
 INSERT INTO erpx_benefits_write.aggregate_versions VALUES(t,k,kind,rid,scope,ver,(d->>'updated_at')::timestamptz)
 ON CONFLICT (tenant_id,aggregate_key) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_benefits_write.aggregate_versions.aggregate_version<EXCLUDED.aggregate_version;
 IF kind='plan' THEN
  INSERT INTO erpx_benefits_write.plans VALUES(t,k,rid,d->>'name',d->>'currency',d->>'eligibility_class',
   (d->>'minimum_tenure_days')::integer,d->'contribution',d->>'status')
  ON CONFLICT (tenant_id,plan_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF kind='enrollment' THEN
  INSERT INTO erpx_benefits_write.enrollments VALUES(t,k,rid,(d->'plan'->>'plan_id')::uuid,(d->'plan'->>'version')::bigint,
   (d->>'window')::uuid,d->'employee',d->'eligibility',d->'life_event',d->'dependents',d->'election',d->'acknowledgements',
   (d->>'payroll_handoff_id')::uuid,d->>'status')
  ON CONFLICT (tenant_id,enrollment_id) DO UPDATE SET life_event=EXCLUDED.life_event,dependents=EXCLUDED.dependents,
   election=EXCLUDED.election,provider_acknowledgements=EXCLUDED.provider_acknowledgements,
   payroll_handoff_id=EXCLUDED.payroll_handoff_id,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF kind IN ('window','handoff') THEN
  INSERT INTO erpx_benefits_write.governed_records VALUES(t,k,rid,kind,d->'subject',d->>'evidence_digest',d->>'status')
  ON CONFLICT(tenant_id,record_id) DO UPDATE SET evidence_digest=EXCLUDED.evidence_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported benefits aggregate kind'; END IF;
END $$;

CREATE VIEW erpx_field_service_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',a.agreement_id,'version',v.aggregate_version,'scope',v.authority_scope,'customer',a.customer_ref,
  'covered_assets',a.covered_assets,'covered_locations',a.covered_locations,'service_codes',a.service_codes,
  'valid_from',a.valid_from,'valid_through',a.valid_through,'active',a.active) AS aggregate_document
FROM erpx_field_service_write.aggregate_versions v JOIN erpx_field_service_write.agreements a ON a.tenant_id=v.tenant_id AND a.aggregate_key=v.aggregate_key
UNION ALL SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',o.service_order_id,'version',v.aggregate_version,'scope',v.authority_scope,'request',o.request_id,
  'request_version',o.request_version,'agreement',o.agreement_id,'agreement_version',o.agreement_version,'customer',o.customer_ref,
  'asset',o.asset_ref,'location',o.location_ref,'service_code',o.service_code,'required_skills',o.required_skills,
  'scheduled_on',o.scheduled_on,'technician',o.technician_qualification,'dispatch_id',o.dispatch_id,'visit_id',o.visit_id,
  'parts',o.part_reservations,'part_fact_ids',o.part_fact_ids,'evidence',o.visit_evidence,'signature',o.customer_signature,
  'exceptions',o.exceptions,'billing_handoff_id',o.billing_handoff_id,'status',o.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_field_service_write.aggregate_versions v JOIN erpx_field_service_write.service_orders o
 ON o.tenant_id=v.tenant_id AND o.aggregate_key=v.aggregate_key
UNION ALL SELECT v.tenant_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.record_id,'version',v.aggregate_version,'scope',v.authority_scope,'kind',r.record_kind,
 'subject',r.subject_ref,'evidence_digest',r.evidence_digest,'status',r.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_field_service_write.aggregate_versions v JOIN erpx_field_service_write.governed_records r ON r.tenant_id=v.tenant_id AND r.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_field_service_write.persist_normalized(uuid,text,text,uuid,jsonb,bigint,jsonb) RETURNS void LANGUAGE plpgsql AS $$
DECLARE t alias for $1; k alias for $2; kind alias for $3; rid alias for $4; scope alias for $5; ver alias for $6; d alias for $7;
BEGIN
 INSERT INTO erpx_field_service_write.aggregate_versions VALUES(t,k,kind,rid,scope,ver,COALESCE((d->>'updated_at')::timestamptz,clock_timestamp()))
 ON CONFLICT (tenant_id,aggregate_key) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_field_service_write.aggregate_versions.aggregate_version<EXCLUDED.aggregate_version;
 IF kind='agreement' THEN
  INSERT INTO erpx_field_service_write.agreements VALUES(t,k,rid,d->'customer',d->'covered_assets',d->'covered_locations',d->'service_codes',
   (d->>'valid_from')::date,(d->>'valid_through')::date,(d->>'active')::boolean)
  ON CONFLICT (tenant_id,agreement_id) DO UPDATE SET active=EXCLUDED.active;
 ELSIF kind='order' THEN
  INSERT INTO erpx_field_service_write.service_orders VALUES(t,k,rid,(d->>'request')::uuid,(d->>'request_version')::bigint,
   (d->>'agreement')::uuid,(d->>'agreement_version')::bigint,d->'customer',d->'asset',d->'location',d->>'service_code',
   d->'required_skills',(d->>'scheduled_on')::date,d->'technician',(d->>'dispatch_id')::uuid,(d->>'visit_id')::uuid,
   d->'parts',d->'part_fact_ids',d->'evidence',d->'signature',d->'exceptions',(d->>'billing_handoff_id')::uuid,d->>'status')
  ON CONFLICT (tenant_id,service_order_id) DO UPDATE SET scheduled_on=EXCLUDED.scheduled_on,
   technician_qualification=EXCLUDED.technician_qualification,dispatch_id=EXCLUDED.dispatch_id,visit_id=EXCLUDED.visit_id,
   part_reservations=EXCLUDED.part_reservations,part_fact_ids=EXCLUDED.part_fact_ids,visit_evidence=EXCLUDED.visit_evidence,
   customer_signature=EXCLUDED.customer_signature,exceptions=EXCLUDED.exceptions,billing_handoff_id=EXCLUDED.billing_handoff_id,
   lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF kind IN ('request','dispatch','visit','handoff') THEN
  INSERT INTO erpx_field_service_write.governed_records VALUES(t,k,rid,kind,d->'subject',d->>'evidence_digest',d->>'status')
  ON CONFLICT(tenant_id,record_id) DO UPDATE SET evidence_digest=EXCLUDED.evidence_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported field-service aggregate kind'; END IF;
END $$;

DO $$
DECLARE relation_name text;
BEGIN
  FOREACH relation_name IN ARRAY ARRAY[
    'erpx_learning_write.aggregate_versions','erpx_learning_write.courses','erpx_learning_write.enrollments','erpx_learning_write.governed_records','erpx_learning_write.attendance_guards','erpx_learning_write.provider_result_guards','erpx_learning_write.completion_facts','erpx_learning_write.command_receipts','erpx_learning_write.audit_log','erpx_learning_write.outbox',
    'erpx_benefits_write.aggregate_versions','erpx_benefits_write.plans','erpx_benefits_write.enrollments','erpx_benefits_write.governed_records','erpx_benefits_write.eligibility_guards','erpx_benefits_write.provider_acknowledgement_guards','erpx_benefits_write.payroll_handoff_guards','erpx_benefits_write.command_receipts','erpx_benefits_write.audit_log','erpx_benefits_write.outbox',
    'erpx_field_service_write.aggregate_versions','erpx_field_service_write.agreements','erpx_field_service_write.service_orders','erpx_field_service_write.governed_records','erpx_field_service_write.part_issue_guards','erpx_field_service_write.visit_evidence_guards','erpx_field_service_write.exception_guards','erpx_field_service_write.billing_handoff_guards','erpx_field_service_write.command_receipts','erpx_field_service_write.audit_log','erpx_field_service_write.outbox'
  ] LOOP
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('CREATE POLICY tenant_isolation ON %s USING (tenant_id = nullif(current_setting(''app.tenant_id'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''app.tenant_id'', true), '''')::uuid)', relation_name);
  END LOOP;
END $$;
