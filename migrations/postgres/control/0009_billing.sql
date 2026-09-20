-- Sandbox service enrollment belongs to ERPX; money belongs to Stripe.
CREATE TABLE billing_plans (
 id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>0), name TEXT NOT NULL,
 support_satang BIGINT NOT NULL CHECK(support_satang BETWEEN 0 AND 100000000),
 effective_month TEXT NOT NULL, created_at BIGINT NOT NULL, PRIMARY KEY(id,revision)
);
INSERT INTO billing_plans VALUES
 ('infrastructure',1,'Infrastructure Only',0,'2026-01',0),
 ('standard',1,'Standard Support',50000,'2026-01',0),
 ('priority',1,'Priority Support',150000,'2026-01',0);
CREATE TABLE billing_accounts (
 holding_id TEXT PRIMARY KEY REFERENCES holding_bindings(holding_id),
 version INTEGER NOT NULL DEFAULT 0, customer_id TEXT UNIQUE,
 payment_mode TEXT NOT NULL DEFAULT 'manual' CHECK(payment_mode IN ('manual','auto')),
 consent_at BIGINT, consent_actor TEXT, card_json TEXT,
 created_at BIGINT NOT NULL
);
CREATE TABLE billing_enrollments (
 id TEXT PRIMARY KEY, holding_id TEXT NOT NULL REFERENCES billing_accounts,
 plan_id TEXT NOT NULL, started_at BIGINT NOT NULL, ended_at BIGINT,
 pending_plan TEXT, pending_end BOOLEAN NOT NULL DEFAULT FALSE, effective_at BIGINT,
 CHECK(NOT(pending_plan IS NOT NULL AND pending_end))
);
CREATE UNIQUE INDEX billing_one_enrollment ON billing_enrollments(holding_id) WHERE ended_at IS NULL;
CREATE TABLE billing_periods (
 id TEXT PRIMARY KEY, holding_id TEXT NOT NULL REFERENCES billing_accounts,
 enrollment_id TEXT NOT NULL REFERENCES billing_enrollments, month TEXT NOT NULL,
 start_at BIGINT NOT NULL, end_at BIGINT NOT NULL, plan_id TEXT NOT NULL, plan_revision INTEGER NOT NULL,
 support_satang BIGINT NOT NULL CHECK(support_satang>=0), costs_complete BOOLEAN NOT NULL DEFAULT FALSE,
 closed_at BIGINT, UNIQUE(enrollment_id,month), FOREIGN KEY(plan_id,plan_revision) REFERENCES billing_plans
);
CREATE TABLE billing_costs (
 id TEXT PRIMARY KEY, holding_id TEXT NOT NULL REFERENCES billing_accounts,
 period_id TEXT NOT NULL REFERENCES billing_periods,
 category TEXT NOT NULL CHECK(category IN ('hosting','storage','provider_ai','adjustment')),
 amount_satang BIGINT NOT NULL CHECK(amount_satang BETWEEN -100000000 AND 100000000),
 service_start BIGINT NOT NULL, service_end BIGINT NOT NULL CHECK(service_end>service_start),
 note TEXT NOT NULL, simulated BOOLEAN NOT NULL DEFAULT FALSE,
 evidence_id TEXT, original_invoice_id TEXT, invoice_id TEXT, created_at BIGINT NOT NULL
);
CREATE TABLE billing_invoices (
 id TEXT PRIMARY KEY, holding_id TEXT NOT NULL REFERENCES billing_accounts,
 period_id TEXT NOT NULL REFERENCES billing_periods, kind TEXT NOT NULL CHECK(kind IN ('main','adjustment')),
 status TEXT NOT NULL DEFAULT 'queued', total_satang BIGINT NOT NULL CHECK(total_satang>=0),
 lines_json TEXT NOT NULL, payment_mode TEXT NOT NULL, stripe_id TEXT UNIQUE,
 hosted_url TEXT, pdf_url TEXT, due_at BIGINT, payment_state TEXT,
 created_at BIGINT NOT NULL, approved_by TEXT NOT NULL, original_invoice_id TEXT REFERENCES billing_invoices,
 updated_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX billing_one_main_invoice ON billing_invoices(period_id) WHERE kind='main' AND status <> 'void';
ALTER TABLE billing_costs ADD FOREIGN KEY(invoice_id) REFERENCES billing_invoices;
ALTER TABLE billing_costs ADD FOREIGN KEY(original_invoice_id) REFERENCES billing_invoices;
CREATE TABLE billing_jobs (
 id TEXT PRIMARY KEY, holding_id TEXT NOT NULL REFERENCES billing_accounts,
 invoice_id TEXT NOT NULL REFERENCES billing_invoices, kind TEXT NOT NULL CHECK(kind IN ('issue','void','manual')),
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','done','attention')),
 progress_json TEXT NOT NULL DEFAULT '{}', attempts INTEGER NOT NULL DEFAULT 0,
 next_at BIGINT NOT NULL, created_at BIGINT NOT NULL, last_error TEXT
);
CREATE TABLE billing_receipts (id TEXT PRIMARY KEY, digest TEXT NOT NULL, result_json TEXT NOT NULL, created_at BIGINT NOT NULL);
CREATE TABLE billing_audit (id TEXT PRIMARY KEY, holding_id TEXT, actor TEXT NOT NULL, action TEXT NOT NULL, reason TEXT NOT NULL, details_json TEXT NOT NULL, created_at BIGINT NOT NULL);
CREATE TABLE billing_evidence (id TEXT PRIMARY KEY, holding_id TEXT NOT NULL REFERENCES billing_accounts, filename TEXT NOT NULL, mime TEXT NOT NULL, size BIGINT NOT NULL, sha256 TEXT NOT NULL, actor TEXT NOT NULL, created_at BIGINT NOT NULL);
ALTER TABLE stripe_inbox ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE stripe_inbox ADD COLUMN next_at BIGINT NOT NULL DEFAULT 0;
ALTER TABLE stripe_inbox ADD COLUMN last_error TEXT;
CREATE INDEX billing_pending_jobs ON billing_jobs(next_at) WHERE state='pending';
CREATE INDEX billing_pending_inbox ON stripe_inbox(next_at) WHERE state='pending';
