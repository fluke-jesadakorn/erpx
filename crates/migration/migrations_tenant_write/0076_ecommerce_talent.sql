-- Normalized E-commerce and Recruiting & Talent write authority. JSON columns
-- hold named value objects/evidence only; no generic aggregate snapshot exists.
CREATE SCHEMA IF NOT EXISTS erpx_ecommerce_write;
CREATE SCHEMA IF NOT EXISTS erpx_talent_write;

CREATE OR REPLACE FUNCTION erpx_ecommerce_write.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;
CREATE OR REPLACE FUNCTION erpx_talent_write.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE TABLE erpx_ecommerce_write.channels (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_tenant_id(),
    channel_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    channel_code TEXT NOT NULL, channel_status TEXT NOT NULL,
    configuration JSONB NOT NULL, proposed_by UUID NOT NULL, approved_by UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, channel_id), UNIQUE (tenant_id, scope_key, channel_code)
);
CREATE TABLE erpx_ecommerce_write.listings (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_tenant_id(),
    listing_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    channel_id UUID NOT NULL, product_ref JSONB NOT NULL, source_versions JSONB NOT NULL,
    unit_price_minor BIGINT NOT NULL CHECK (unit_price_minor >= 0), unit_price_currency TEXT NOT NULL,
    available BOOLEAN NOT NULL, listing_status TEXT NOT NULL,
    proposed_by UUID NOT NULL, approved_by UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, listing_id),
    FOREIGN KEY (tenant_id, channel_id) REFERENCES erpx_ecommerce_write.channels(tenant_id, channel_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_ecommerce_write.carts (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_tenant_id(),
    cart_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    channel_id UUID NOT NULL, owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')),
    owner_principal_id UUID NOT NULL, verified_customer_ref JSONB,
    marketing_consent BOOLEAN NOT NULL, cart_status TEXT NOT NULL, lines JSONB NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, cart_id),
    FOREIGN KEY (tenant_id, channel_id) REFERENCES erpx_ecommerce_write.channels(tenant_id, channel_id) ON DELETE RESTRICT,
    CHECK (owner_realm <> 'guest' OR verified_customer_ref IS NULL)
);
CREATE TABLE erpx_ecommerce_write.checkouts (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_tenant_id(),
    checkout_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    idempotency_key TEXT NOT NULL, cart_id UUID NOT NULL, cart_version BIGINT NOT NULL CHECK (cart_version > 0),
    owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')), owner_principal_id UUID NOT NULL,
    checkout_snapshot JSONB NOT NULL, revalidation JSONB NOT NULL, reservation JSONB NOT NULL, payment JSONB NOT NULL,
    order_receipt TEXT, order_ref JSONB, checkout_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, checkout_id), UNIQUE (tenant_id, scope_key, idempotency_key),
    FOREIGN KEY (tenant_id, cart_id) REFERENCES erpx_ecommerce_write.carts(tenant_id, cart_id) ON DELETE RESTRICT,
    CHECK ((order_receipt IS NULL) = (order_ref IS NULL))
);
CREATE TABLE erpx_ecommerce_write.returns (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_tenant_id(),
    return_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    checkout_id UUID NOT NULL, order_ref JSONB NOT NULL,
    owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')), owner_principal_id UUID NOT NULL,
    owner_ref JSONB NOT NULL, reason TEXT NOT NULL, evidence_digest TEXT NOT NULL,
    handoff_receipt TEXT, refund_receipt TEXT, return_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, return_id),
    FOREIGN KEY (tenant_id, checkout_id) REFERENCES erpx_ecommerce_write.checkouts(tenant_id, checkout_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_ecommerce_write.fulfillment_requests (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_tenant_id(),
    fulfillment_request_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')), owner_principal_id UUID NOT NULL,
    owner_ref JSONB NOT NULL, checkout_ref JSONB NOT NULL, order_ref JSONB NOT NULL,
    reservation_digest TEXT NOT NULL CHECK (reservation_digest ~ '^[0-9A-Fa-f]{64}$'),
    fulfillment_status TEXT NOT NULL CHECK (fulfillment_status IN ('proposed','accepted','dispatched','completed','rejected','cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,fulfillment_request_id)
);
CREATE TABLE erpx_ecommerce_write.command_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL, command_id UUID NOT NULL, request_digest TEXT NOT NULL,
    aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant_id, idempotency_key), UNIQUE (tenant_id, command_id)
);
CREATE TABLE erpx_ecommerce_write.audit_log (
    tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_tenant_id(),
    audit_id UUID NOT NULL, command_id UUID NOT NULL, actor_principal_id UUID NOT NULL,
    action TEXT NOT NULL, aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), scope_key TEXT NOT NULL,
    correlation_id UUID NOT NULL, evidence_digest TEXT NOT NULL, occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, audit_id), UNIQUE (tenant_id, command_id)
);
CREATE TABLE erpx_ecommerce_write.outbox (
    position BIGINT GENERATED ALWAYS AS IDENTITY, tenant_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_tenant_id(),
    event_id UUID NOT NULL, aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), event_type TEXT NOT NULL,
    correlation_id UUID NOT NULL, payload JSONB NOT NULL, occurred_at TIMESTAMPTZ NOT NULL, published_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, position), UNIQUE (tenant_id, event_id), UNIQUE (tenant_id, aggregate_id, aggregate_version)
);

CREATE VIEW erpx_ecommerce_write.aggregate_rehydration WITH (security_invoker = true) AS
SELECT tenant_id,'commerce_channel'::text AS aggregate_type,channel_id AS aggregate_id,aggregate_version,
       jsonb_build_object('id',channel_id,'version',aggregate_version,'scope',tenant_scope,'code',channel_code,
       'locales',configuration->'locales','currencies',configuration->'currencies','status',channel_status,
       'proposed_by',proposed_by,'approved_by',approved_by,'updated_at',updated_at) AS aggregate_document
FROM erpx_ecommerce_write.channels
UNION ALL SELECT tenant_id,'commerce_listing',listing_id,aggregate_version,
       jsonb_build_object('id',listing_id,'version',aggregate_version,'scope',tenant_scope,'channel_id',channel_id,
       'product',product_ref,'source_versions',source_versions,'unit_price',jsonb_build_object('minor_units',unit_price_minor,'currency',unit_price_currency),
       'available',available,'status',listing_status,'proposed_by',proposed_by,'approved_by',approved_by,'updated_at',updated_at)
FROM erpx_ecommerce_write.listings
UNION ALL SELECT tenant_id,'commerce_cart',cart_id,aggregate_version,
       jsonb_build_object('id',cart_id,'version',aggregate_version,'scope',tenant_scope,'channel_id',channel_id,
       'owner',jsonb_build_object('realm',owner_realm,'principal_id',owner_principal_id,'verified_customer',verified_customer_ref),
       'marketing_consent',marketing_consent,'lines',lines,'status',cart_status,'updated_at',updated_at)
FROM erpx_ecommerce_write.carts
UNION ALL SELECT tenant_id,'commerce_checkout',checkout_id,aggregate_version,
       jsonb_build_object('id',checkout_id,'version',aggregate_version,'scope',tenant_scope,'idempotency_key',idempotency_key,
       'snapshot',checkout_snapshot,'revalidation',revalidation,'reservation',reservation,'payment',payment,
       'order_receipt',order_receipt,'order_ref',order_ref,'status',checkout_status,'updated_at',updated_at)
FROM erpx_ecommerce_write.checkouts
UNION ALL SELECT tenant_id,'commerce_return',return_id,aggregate_version,
       jsonb_build_object('id',return_id,'version',aggregate_version,'scope',tenant_scope,'checkout_id',checkout_id,
       'order_ref',order_ref,'owner',owner_ref,'reason',reason,'evidence_digest',evidence_digest,
       'handoff_receipt',handoff_receipt,'refund_receipt',refund_receipt,'status',return_status,'updated_at',updated_at)
FROM erpx_ecommerce_write.returns
UNION ALL SELECT tenant_id,'commerce_fulfillment_request',fulfillment_request_id,aggregate_version,
       jsonb_build_object('id',fulfillment_request_id,'version',aggregate_version,'scope',tenant_scope,'owner',owner_ref,
       'checkout',checkout_ref,'order',order_ref,'reservation_digest',reservation_digest,
       'status',fulfillment_status,'updated_at',updated_at)
FROM erpx_ecommerce_write.fulfillment_requests;

CREATE FUNCTION erpx_ecommerce_write.persist_normalized(
    p_tenant_id UUID, p_aggregate_type TEXT, p_aggregate_id UUID,
    p_scope_key TEXT, p_tenant_scope JSONB, s JSONB
) RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
    IF p_aggregate_type = 'commerce_channel' THEN
        INSERT INTO erpx_ecommerce_write.channels
            (tenant_id,channel_id,scope_key,tenant_scope,channel_code,channel_status,configuration,proposed_by,approved_by,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,s->>'code',s->>'status',
            jsonb_build_object('locales',s->'locales','currencies',s->'currencies'),
            (s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,channel_id) DO UPDATE SET channel_status=EXCLUDED.channel_status,
            configuration=EXCLUDED.configuration,approved_by=EXCLUDED.approved_by,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_listing' THEN
        INSERT INTO erpx_ecommerce_write.listings
            (tenant_id,listing_id,scope_key,tenant_scope,channel_id,product_ref,source_versions,unit_price_minor,unit_price_currency,available,listing_status,proposed_by,approved_by,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,(s->>'channel_id')::uuid,s->'product',s->'source_versions',
            (s->'unit_price'->>'minor_units')::bigint,s->'unit_price'->>'currency',(s->>'available')::boolean,s->>'status',
            (s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,listing_id) DO UPDATE SET source_versions=EXCLUDED.source_versions,
            available=EXCLUDED.available,listing_status=EXCLUDED.listing_status,approved_by=EXCLUDED.approved_by,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_cart' THEN
        INSERT INTO erpx_ecommerce_write.carts
            (tenant_id,cart_id,scope_key,tenant_scope,channel_id,owner_realm,owner_principal_id,verified_customer_ref,marketing_consent,cart_status,lines,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,(s->>'channel_id')::uuid,s->'owner'->>'realm',
            (s->'owner'->>'principal_id')::uuid,s->'owner'->'verified_customer',(s->>'marketing_consent')::boolean,
            s->>'status',s->'lines',(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,cart_id) DO UPDATE SET owner_realm=EXCLUDED.owner_realm,
            owner_principal_id=EXCLUDED.owner_principal_id,verified_customer_ref=EXCLUDED.verified_customer_ref,
            marketing_consent=EXCLUDED.marketing_consent,cart_status=EXCLUDED.cart_status,lines=EXCLUDED.lines,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_checkout' THEN
        INSERT INTO erpx_ecommerce_write.checkouts
            (tenant_id,checkout_id,scope_key,tenant_scope,idempotency_key,cart_id,cart_version,owner_realm,owner_principal_id,checkout_snapshot,revalidation,reservation,payment,order_receipt,order_ref,checkout_status,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,s->>'idempotency_key',(s->'snapshot'->>'cart_id')::uuid,
            (s->'snapshot'->>'cart_version')::bigint,s->'snapshot'->'owner'->>'realm',(s->'snapshot'->'owner'->>'principal_id')::uuid,
            s->'snapshot',s->'revalidation',s->'reservation',s->'payment',s->>'order_receipt',s->'order_ref',s->>'status',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,checkout_id) DO UPDATE SET revalidation=EXCLUDED.revalidation,
            reservation=EXCLUDED.reservation,payment=EXCLUDED.payment,order_receipt=EXCLUDED.order_receipt,
            order_ref=EXCLUDED.order_ref,checkout_status=EXCLUDED.checkout_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_return' THEN
        INSERT INTO erpx_ecommerce_write.returns
            (tenant_id,return_id,scope_key,tenant_scope,checkout_id,order_ref,owner_realm,owner_principal_id,owner_ref,reason,evidence_digest,handoff_receipt,refund_receipt,return_status,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,(s->>'checkout_id')::uuid,s->'order_ref',s->'owner'->>'realm',
            (s->'owner'->>'principal_id')::uuid,s->'owner',s->>'reason',s->>'evidence_digest',s->>'handoff_receipt',
            s->>'refund_receipt',s->>'status',(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,return_id) DO UPDATE SET handoff_receipt=EXCLUDED.handoff_receipt,
            refund_receipt=EXCLUDED.refund_receipt,return_status=EXCLUDED.return_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_fulfillment_request' THEN
        INSERT INTO erpx_ecommerce_write.fulfillment_requests
            (tenant_id,fulfillment_request_id,scope_key,tenant_scope,owner_realm,owner_principal_id,owner_ref,
             checkout_ref,order_ref,reservation_digest,fulfillment_status,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,s->'owner'->>'realm',(s->'owner'->>'principal_id')::uuid,
            s->'owner',s->'checkout',s->'order',s->>'reservation_digest',s->>'status',(s->>'version')::bigint,
            (s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,fulfillment_request_id) DO UPDATE SET fulfillment_status=EXCLUDED.fulfillment_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSE
        RAISE EXCEPTION 'unsupported commerce aggregate type';
    END IF;
END;
$$;

CREATE TABLE erpx_talent_write.requisitions (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    requisition_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    title TEXT NOT NULL, position_ref JSONB NOT NULL, position_version BIGINT NOT NULL,
    budget_ref JSONB NOT NULL, budget_version BIGINT NOT NULL, publication_evidence JSONB NOT NULL,
    proposed_by UUID NOT NULL, approved_by UUID, requisition_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, requisition_id)
);
CREATE TABLE erpx_talent_write.candidates (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    candidate_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    authority_realm TEXT NOT NULL CHECK (authority_realm = 'candidate'), candidate_principal_id UUID NOT NULL,
    profile_digest TEXT NOT NULL, attachment_digests JSONB NOT NULL,
    recruiting_consent BOOLEAN NOT NULL, marketing_consent BOOLEAN NOT NULL, legal_hold BOOLEAN NOT NULL,
    retention_policy_version BIGINT NOT NULL, candidate_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, candidate_id), UNIQUE (tenant_id, candidate_principal_id)
);
CREATE TABLE erpx_talent_write.applications (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    application_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    authority_realm TEXT NOT NULL CHECK (authority_realm = 'candidate'),
    candidate_id UUID NOT NULL, candidate_principal_id UUID NOT NULL, profile_version BIGINT NOT NULL,
    requisition_id UUID NOT NULL, requisition_version BIGINT NOT NULL, submission_idempotency_key TEXT NOT NULL,
    interviews JSONB NOT NULL, selection_evidence JSONB, application_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, application_id),
    UNIQUE (tenant_id, candidate_id, requisition_id, submission_idempotency_key),
    FOREIGN KEY (tenant_id, candidate_id) REFERENCES erpx_talent_write.candidates(tenant_id, candidate_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, requisition_id) REFERENCES erpx_talent_write.requisitions(tenant_id, requisition_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_talent_write.offers (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    offer_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    application_id UUID NOT NULL, application_version BIGINT NOT NULL,
    candidate_principal_id UUID NOT NULL, terms_digest TEXT NOT NULL, terms_version BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL, proposed_by UUID NOT NULL, approved_by UUID,
    acceptance_receipt TEXT, offer_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, offer_id),
    FOREIGN KEY (tenant_id, application_id) REFERENCES erpx_talent_write.applications(tenant_id, application_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_talent_write.hire_handoffs (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    handoff_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    offer_id UUID NOT NULL, offer_version BIGINT NOT NULL, idempotency_key TEXT NOT NULL,
    evidence_digest TEXT NOT NULL, handoff_status TEXT NOT NULL,
    owner_receipt TEXT, employee_ref JSONB,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, handoff_id), UNIQUE (tenant_id, scope_key, idempotency_key),
    FOREIGN KEY (tenant_id, offer_id) REFERENCES erpx_talent_write.offers(tenant_id, offer_id) ON DELETE RESTRICT,
    CHECK (handoff_status <> 'accepted' OR (owner_receipt IS NOT NULL AND employee_ref IS NOT NULL))
);
CREATE TABLE erpx_talent_write.proposals (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    proposal_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    authority_realm TEXT NOT NULL CHECK (authority_realm = 'workforce'),
    proposal_kind TEXT NOT NULL, employee_ref JSONB NOT NULL,
    employee_version BIGINT NOT NULL, cycle_version BIGINT NOT NULL, evidence_digest TEXT NOT NULL,
    amount JSONB, proposed_by UUID NOT NULL, approved_by UUID, proposal_status TEXT NOT NULL, owner_receipt TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, proposal_id), CHECK ((proposal_kind = 'compensation') = (amount IS NOT NULL))
);
CREATE TABLE erpx_talent_write.governed_records (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    record_id UUID NOT NULL, scope_key TEXT NOT NULL, tenant_scope JSONB NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('talent_interview','talent_review_cycle')),
    authority_realm TEXT NOT NULL CHECK (authority_realm IN ('candidate','workforce')),
    candidate_principal_id UUID, subject_ref JSONB NOT NULL,
    evidence_digest TEXT NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN ('draft','scheduled','in_progress','completed','closed','cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id,record_id),
    CHECK ((record_kind='talent_interview' AND authority_realm='candidate' AND candidate_principal_id IS NOT NULL)
        OR (record_kind='talent_review_cycle' AND authority_realm='workforce' AND candidate_principal_id IS NULL))
);
CREATE TABLE erpx_talent_write.command_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL, command_id UUID NOT NULL, request_digest TEXT NOT NULL,
    aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (tenant_id, idempotency_key), UNIQUE (tenant_id, command_id)
);
CREATE TABLE erpx_talent_write.audit_log (
    tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    audit_id UUID NOT NULL, command_id UUID NOT NULL, actor_principal_id UUID NOT NULL,
    actor_realm TEXT NOT NULL CHECK (actor_realm IN ('candidate','workforce')),
    action TEXT NOT NULL, aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), scope_key TEXT NOT NULL,
    correlation_id UUID NOT NULL, evidence_digest TEXT NOT NULL, occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, audit_id), UNIQUE (tenant_id, command_id)
);
CREATE TABLE erpx_talent_write.outbox (
    position BIGINT GENERATED ALWAYS AS IDENTITY, tenant_id UUID NOT NULL DEFAULT erpx_talent_write.current_tenant_id(),
    event_id UUID NOT NULL, aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), event_type TEXT NOT NULL,
    correlation_id UUID NOT NULL, payload JSONB NOT NULL, occurred_at TIMESTAMPTZ NOT NULL, published_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, position), UNIQUE (tenant_id, event_id), UNIQUE (tenant_id, aggregate_id, aggregate_version)
);

CREATE VIEW erpx_talent_write.aggregate_rehydration WITH (security_invoker = true) AS
SELECT tenant_id,'talent_requisition'::text AS aggregate_type,requisition_id AS aggregate_id,aggregate_version,
       jsonb_build_object('id',requisition_id,'version',aggregate_version,'scope',tenant_scope,'title',title,
       'position',position_ref,'position_version',position_version,'budget',budget_ref,'budget_version',budget_version,
       'public_content_digest',publication_evidence->>'public_content_digest','privacy_review_version',publication_evidence->'privacy_review_version',
       'proposed_by',proposed_by,'approved_by',approved_by,'status',requisition_status,'updated_at',updated_at) AS aggregate_document
FROM erpx_talent_write.requisitions
UNION ALL SELECT tenant_id,'talent_candidate',candidate_id,aggregate_version,
       jsonb_build_object('id',candidate_id,'version',aggregate_version,'scope',tenant_scope,
       'principal',jsonb_build_object('realm',authority_realm,'principal_id',candidate_principal_id),
       'profile_digest',profile_digest,'attachment_digests',attachment_digests,'recruiting_consent',recruiting_consent,
       'marketing_consent',marketing_consent,'legal_hold',legal_hold,'retention_policy_version',retention_policy_version,
       'status',candidate_status,'updated_at',updated_at)
FROM erpx_talent_write.candidates
UNION ALL SELECT tenant_id,'talent_application',application_id,aggregate_version,
       jsonb_build_object('id',application_id,'version',aggregate_version,'scope',tenant_scope,'candidate_id',candidate_id,
       'candidate_principal_id',candidate_principal_id,'profile_version',profile_version,'requisition_id',requisition_id,
       'requisition_version',requisition_version,'idempotency_key',submission_idempotency_key,'interviews',interviews,
       'selection',selection_evidence,'status',application_status,'updated_at',updated_at)
FROM erpx_talent_write.applications
UNION ALL SELECT tenant_id,'talent_offer',offer_id,aggregate_version,
       jsonb_build_object('id',offer_id,'version',aggregate_version,'scope',tenant_scope,'application_id',application_id,
       'application_version',application_version,'candidate_principal_id',candidate_principal_id,'terms_digest',terms_digest,
       'terms_version',terms_version,'expires_at',expires_at,'proposed_by',proposed_by,'approved_by',approved_by,
       'acceptance_receipt',acceptance_receipt,'status',offer_status,'updated_at',updated_at)
FROM erpx_talent_write.offers
UNION ALL SELECT tenant_id,'talent_hire_handoff',handoff_id,aggregate_version,
       jsonb_build_object('id',handoff_id,'version',aggregate_version,'scope',tenant_scope,'offer_id',offer_id,
       'offer_version',offer_version,'idempotency_key',idempotency_key,'evidence_digest',evidence_digest,
       'status',handoff_status,'owner_receipt',owner_receipt,'employee_ref',employee_ref,'updated_at',updated_at)
FROM erpx_talent_write.hire_handoffs
UNION ALL SELECT tenant_id,'talent_proposal',proposal_id,aggregate_version,
       jsonb_build_object('id',proposal_id,'version',aggregate_version,'scope',tenant_scope,'kind',proposal_kind,
       'employee',employee_ref,'employee_version',employee_version,'cycle_version',cycle_version,'evidence_digest',evidence_digest,
       'amount',amount,'proposed_by',proposed_by,'approved_by',approved_by,'status',proposal_status,
       'owner_receipt',owner_receipt,'updated_at',updated_at)
FROM erpx_talent_write.proposals
UNION ALL SELECT tenant_id,record_kind,record_id,aggregate_version,
       jsonb_build_object('id',record_id,'version',aggregate_version,'scope',tenant_scope,
       'kind',CASE record_kind WHEN 'talent_interview' THEN 'interview' ELSE 'review_cycle' END,
       'authority_realm',authority_realm,'candidate_principal_id',candidate_principal_id,'subject',subject_ref,
       'evidence_digest',evidence_digest,'status',lifecycle_state,'updated_at',updated_at)
FROM erpx_talent_write.governed_records;

CREATE FUNCTION erpx_talent_write.persist_normalized(
    p_tenant_id UUID, p_aggregate_type TEXT, p_aggregate_id UUID,
    p_scope_key TEXT, p_tenant_scope JSONB, s JSONB
) RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
    IF p_aggregate_type = 'talent_requisition' THEN
        INSERT INTO erpx_talent_write.requisitions
            (tenant_id,requisition_id,scope_key,tenant_scope,title,position_ref,position_version,budget_ref,budget_version,
             publication_evidence,proposed_by,approved_by,requisition_status,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,s->>'title',s->'position',(s->>'position_version')::bigint,
            s->'budget',(s->>'budget_version')::bigint,jsonb_build_object('public_content_digest',s->'public_content_digest',
            'privacy_review_version',s->'privacy_review_version'),(s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,
            s->>'status',(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,requisition_id) DO UPDATE SET approved_by=EXCLUDED.approved_by,
            requisition_status=EXCLUDED.requisition_status,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_candidate' THEN
        INSERT INTO erpx_talent_write.candidates
            (tenant_id,candidate_id,scope_key,tenant_scope,authority_realm,candidate_principal_id,profile_digest,attachment_digests,
             recruiting_consent,marketing_consent,legal_hold,retention_policy_version,candidate_status,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,s->'principal'->>'realm',(s->'principal'->>'principal_id')::uuid,
            s->>'profile_digest',s->'attachment_digests',(s->>'recruiting_consent')::boolean,(s->>'marketing_consent')::boolean,
            (s->>'legal_hold')::boolean,(s->>'retention_policy_version')::bigint,s->>'status',(s->>'version')::bigint,
            (s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,candidate_id) DO UPDATE SET profile_digest=EXCLUDED.profile_digest,
            attachment_digests=EXCLUDED.attachment_digests,recruiting_consent=EXCLUDED.recruiting_consent,
            marketing_consent=EXCLUDED.marketing_consent,legal_hold=EXCLUDED.legal_hold,candidate_status=EXCLUDED.candidate_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_application' THEN
        INSERT INTO erpx_talent_write.applications
            (tenant_id,application_id,scope_key,tenant_scope,authority_realm,candidate_id,candidate_principal_id,profile_version,
             requisition_id,requisition_version,submission_idempotency_key,interviews,selection_evidence,application_status,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,'candidate',(s->>'candidate_id')::uuid,
            (s->>'candidate_principal_id')::uuid,(s->>'profile_version')::bigint,(s->>'requisition_id')::uuid,
            (s->>'requisition_version')::bigint,s->>'idempotency_key',s->'interviews',s->'selection',s->>'status',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,application_id) DO UPDATE SET interviews=EXCLUDED.interviews,
            selection_evidence=EXCLUDED.selection_evidence,application_status=EXCLUDED.application_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_offer' THEN
        INSERT INTO erpx_talent_write.offers
            (tenant_id,offer_id,scope_key,tenant_scope,application_id,application_version,candidate_principal_id,terms_digest,
             terms_version,expires_at,proposed_by,approved_by,acceptance_receipt,offer_status,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,(s->>'application_id')::uuid,(s->>'application_version')::bigint,
            (s->>'candidate_principal_id')::uuid,s->>'terms_digest',(s->>'terms_version')::bigint,(s->>'expires_at')::timestamptz,
            (s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,s->>'acceptance_receipt',s->>'status',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,offer_id) DO UPDATE SET approved_by=EXCLUDED.approved_by,
            acceptance_receipt=EXCLUDED.acceptance_receipt,offer_status=EXCLUDED.offer_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_hire_handoff' THEN
        INSERT INTO erpx_talent_write.hire_handoffs
            (tenant_id,handoff_id,scope_key,tenant_scope,offer_id,offer_version,idempotency_key,evidence_digest,
             handoff_status,owner_receipt,employee_ref,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,(s->>'offer_id')::uuid,(s->>'offer_version')::bigint,
            s->>'idempotency_key',s->>'evidence_digest',s->>'status',s->>'owner_receipt',s->'employee_ref',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,handoff_id) DO UPDATE SET handoff_status=EXCLUDED.handoff_status,
            owner_receipt=EXCLUDED.owner_receipt,employee_ref=EXCLUDED.employee_ref,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_proposal' THEN
        INSERT INTO erpx_talent_write.proposals
            (tenant_id,proposal_id,scope_key,tenant_scope,authority_realm,proposal_kind,employee_ref,employee_version,
             cycle_version,evidence_digest,amount,proposed_by,approved_by,proposal_status,owner_receipt,aggregate_version,updated_at)
        VALUES (p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,'workforce',s->>'kind',s->'employee',
            (s->>'employee_version')::bigint,(s->>'cycle_version')::bigint,s->>'evidence_digest',s->'amount',
            (s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,s->>'status',s->>'owner_receipt',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,proposal_id) DO UPDATE SET approved_by=EXCLUDED.approved_by,
            proposal_status=EXCLUDED.proposal_status,owner_receipt=EXCLUDED.owner_receipt,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type IN ('talent_interview','talent_review_cycle') THEN
        INSERT INTO erpx_talent_write.governed_records
            (tenant_id,record_id,scope_key,tenant_scope,record_kind,authority_realm,candidate_principal_id,
             subject_ref,evidence_digest,lifecycle_state,aggregate_version,updated_at)
        VALUES(p_tenant_id,p_aggregate_id,p_scope_key,p_tenant_scope,p_aggregate_type,s->>'authority_realm',
            (s->>'candidate_principal_id')::uuid,s->'subject',s->>'evidence_digest',s->>'status',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (tenant_id,record_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,
            evidence_digest=EXCLUDED.evidence_digest,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSE
        RAISE EXCEPTION 'unsupported talent aggregate type';
    END IF;
END;
$$;

CREATE INDEX ecommerce_scope_status ON erpx_ecommerce_write.checkouts(tenant_id, scope_key, checkout_status, updated_at DESC);
CREATE INDEX ecommerce_uncertain_checkout ON erpx_ecommerce_write.checkouts(tenant_id, scope_key, payment) WHERE payment->>'status' = 'uncertain';
CREATE INDEX talent_candidate_owner ON erpx_talent_write.applications(tenant_id, candidate_principal_id, application_id);
CREATE INDEX talent_handoff_uncertain ON erpx_talent_write.hire_handoffs(tenant_id, scope_key, handoff_status) WHERE handoff_status = 'uncertain';

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_ecommerce_write','channels'),('erpx_ecommerce_write','listings'),
        ('erpx_ecommerce_write','carts'),('erpx_ecommerce_write','checkouts'),
        ('erpx_ecommerce_write','returns'),('erpx_ecommerce_write','fulfillment_requests'),('erpx_ecommerce_write','command_receipts'),
        ('erpx_ecommerce_write','audit_log'),('erpx_ecommerce_write','outbox'),
        ('erpx_talent_write','requisitions'),('erpx_talent_write','candidates'),
        ('erpx_talent_write','applications'),('erpx_talent_write','offers'),
        ('erpx_talent_write','hire_handoffs'),('erpx_talent_write','proposals'),('erpx_talent_write','governed_records'),
        ('erpx_talent_write','command_receipts'),('erpx_talent_write','audit_log'),
        ('erpx_talent_write','outbox')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I.%I USING (tenant_id = %I.current_tenant_id()) '
            'WITH CHECK (tenant_id = %I.current_tenant_id())',
            schema_name, table_name, schema_name, schema_name
        );
    END LOOP;
END;
$$;
