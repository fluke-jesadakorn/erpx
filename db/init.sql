-- Enable Vector Extension
CREATE EXTENSION IF NOT EXISTS vector;

-- 1. Roles & Users
CREATE TABLE IF NOT EXISTS roles (
    id SERIAL PRIMARY KEY,
    name VARCHAR(50) UNIQUE NOT NULL -- 'staff', 'accountant', 'manager', 'admin'
);

CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    employee_code VARCHAR(20) UNIQUE NOT NULL,
    fullname VARCHAR(100) NOT NULL,
    line_user_id VARCHAR(100) UNIQUE,
    role_id INT REFERENCES roles(id),
    department VARCHAR(50),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 2. Chart of Accounts (COA)
CREATE TABLE IF NOT EXISTS chart_of_accounts (
    code VARCHAR(20) PRIMARY KEY, -- e.g. '510200'
    name VARCHAR(100) NOT NULL,    -- e.g. 'Travel & Transportation'
    name_th VARCHAR(100) NOT NULL, -- e.g. 'Travel and Transportation Expenses'
    account_type VARCHAR(50) NOT NULL, -- 'expense', 'asset', 'liability'
    embedding vector(1024)         -- Embedding using BGE-M3 (1024 dimensions)
);

-- 3. Expenses (Header)
CREATE TABLE IF NOT EXISTS expenses (
    id SERIAL PRIMARY KEY,
    submitter_id INT REFERENCES users(id),
    vendor_name VARCHAR(150),
    transaction_date DATE,
    subtotal DECIMAL(12, 2) DEFAULT 0.00,
    vat_amount DECIMAL(12, 2) DEFAULT 0.00,
    total_amount DECIMAL(12, 2) DEFAULT 0.00,
    payment_method VARCHAR(50), -- 'cash', 'credit_card', 'transfer'
    status VARCHAR(50) DEFAULT 'draft', -- 'draft', 'ocr_extracted', 'accountant_reviewed', 'approved', 'paid', 'rejected'
    ocr_raw_json JSONB,
    is_corrupted BOOLEAN DEFAULT FALSE,
    correction_notes TEXT,
    document_url VARCHAR(255), -- PDF or Image stored in MinIO
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 4. Expense Items (Details)
CREATE TABLE IF NOT EXISTS expense_items (
    id SERIAL PRIMARY KEY,
    expense_id INT REFERENCES expenses(id) ON DELETE CASCADE,
    description VARCHAR(255) NOT NULL,
    amount DECIMAL(12, 2) NOT NULL,
    mapped_account_code VARCHAR(20) REFERENCES chart_of_accounts(code),
    confidence_score FLOAT, -- Match confidence from cosine similarity
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 5. Approval & Verification Logs
CREATE TABLE IF NOT EXISTS approval_logs (
    id SERIAL PRIMARY KEY,
    expense_id INT REFERENCES expenses(id) ON DELETE CASCADE,
    actor_id INT REFERENCES users(id),
    previous_status VARCHAR(50),
    new_status VARCHAR(50),
    comments TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
