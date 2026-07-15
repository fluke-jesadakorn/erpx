-- v2 schema additions for Folio
-- Additive only: does not modify existing v1 tables

-- 1. Departments
CREATE TABLE IF NOT EXISTS departments (
    id SERIAL PRIMARY KEY,
    code VARCHAR(20) UNIQUE NOT NULL,
    name VARCHAR(100) NOT NULL,
    head_user_id INT REFERENCES users(id) ON DELETE SET NULL,
    monthly_budget DECIMAL(14, 2) DEFAULT 0.00,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 2. Slip / file storage metadata
CREATE TABLE IF NOT EXISTS slips (
    id SERIAL PRIMARY KEY,
    expense_id INT REFERENCES expenses(id) ON DELETE SET NULL,
    pr_id INT,
    file_path VARCHAR(500) NOT NULL,
    mime_type VARCHAR(100) NOT NULL,
    file_size INT NOT NULL,
    ocr_raw_json JSONB,
    ocr_confidence FLOAT,
    ai_reasoning TEXT,
    uploaded_by INT REFERENCES users(id),
    uploaded_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_slips_expense ON slips(expense_id);
CREATE INDEX IF NOT EXISTS idx_slips_pr ON slips(pr_id);

-- 3. Purchase Requisitions (PR)
CREATE TABLE IF NOT EXISTS purchase_requisitions (
    id SERIAL PRIMARY KEY,
    requester_id INT REFERENCES users(id),
    department_id INT REFERENCES departments(id),
    vendor_name VARCHAR(150),
    need_by_date DATE,
    status VARCHAR(50) DEFAULT 'draft',
    total_estimate DECIMAL(14, 2) DEFAULT 0.00,
    currency VARCHAR(10) DEFAULT 'THB',
    justification TEXT,
    document_url VARCHAR(500),
    is_recurring BOOLEAN DEFAULT FALSE,
    matched_policy_id INT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_pr_status ON purchase_requisitions(status);
CREATE INDEX IF NOT EXISTS idx_pr_dept ON purchase_requisitions(department_id);

-- 4. PR Items
CREATE TABLE IF NOT EXISTS pr_items (
    id SERIAL PRIMARY KEY,
    pr_id INT REFERENCES purchase_requisitions(id) ON DELETE CASCADE,
    description VARCHAR(255) NOT NULL,
    qty DECIMAL(12, 2) NOT NULL DEFAULT 1.00,
    unit_price DECIMAL(12, 2) NOT NULL DEFAULT 0.00,
    mapped_account_code VARCHAR(20) REFERENCES chart_of_accounts(code),
    confidence_score FLOAT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 5. Approval Policies (CFO-editable rules)
CREATE TABLE IF NOT EXISTS approval_policies (
    id SERIAL PRIMARY KEY,
    name VARCHAR(150) NOT NULL,
    priority INT NOT NULL DEFAULT 100,
    is_active BOOLEAN DEFAULT TRUE,
    target_type VARCHAR(20) DEFAULT 'expense',
    conditions_json JSONB NOT NULL,
    action_json JSONB NOT NULL,
    created_by INT REFERENCES users(id),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_policies_priority ON approval_policies(priority) WHERE is_active = TRUE;

-- 6. Policy audit trail
CREATE TABLE IF NOT EXISTS policy_audit (
    id SERIAL PRIMARY KEY,
    policy_id INT REFERENCES approval_policies(id) ON DELETE CASCADE,
    actor_id INT REFERENCES users(id),
    before_json JSONB,
    after_json JSONB,
    changed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 7. CEO overrides
CREATE TABLE IF NOT EXISTS ceo_overrides (
    id SERIAL PRIMARY KEY,
    target_type VARCHAR(20) NOT NULL,
    target_id INT NOT NULL,
    actor_id INT REFERENCES users(id),
    reason TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_ceo_target ON ceo_overrides(target_type, target_id);

-- 8. Notifications
CREATE TABLE IF NOT EXISTS notifications (
    id SERIAL PRIMARY KEY,
    user_id INT REFERENCES users(id) ON DELETE CASCADE,
    type VARCHAR(50) NOT NULL,
    target_type VARCHAR(20),
    target_id INT,
    payload_json JSONB,
    read_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_notif_user_unread ON notifications(user_id, read_at);

-- 9. Approval log extensions
ALTER TABLE approval_logs
    ADD COLUMN IF NOT EXISTS stage VARCHAR(50),
    ADD COLUMN IF NOT EXISTS chain_index INT;

-- 10. Deferred FKs (added after all tables exist)
ALTER TABLE slips
    DROP CONSTRAINT IF EXISTS fk_slips_pr;
ALTER TABLE slips
    ADD CONSTRAINT fk_slips_pr
    FOREIGN KEY (pr_id) REFERENCES purchase_requisitions(id) ON DELETE SET NULL;

ALTER TABLE purchase_requisitions
    DROP CONSTRAINT IF EXISTS fk_pr_matched_policy;
ALTER TABLE purchase_requisitions
    ADD CONSTRAINT fk_pr_matched_policy
    FOREIGN KEY (matched_policy_id) REFERENCES approval_policies(id) ON DELETE SET NULL;
