-- Create Journal Entries table
CREATE TABLE IF NOT EXISTS journal_entries (
    id SERIAL PRIMARY KEY,
    expense_id INT REFERENCES expenses(id) ON DELETE SET NULL,
    entry_date DATE NOT NULL DEFAULT CURRENT_DATE,
    description VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create Ledger Lines table
CREATE TABLE IF NOT EXISTS ledger_lines (
    id SERIAL PRIMARY KEY,
    journal_entry_id INT REFERENCES journal_entries(id) ON DELETE CASCADE,
    account_code VARCHAR(20) REFERENCES chart_of_accounts(code),
    debit DECIMAL(12, 2) NOT NULL DEFAULT 0.00,
    credit DECIMAL(12, 2) NOT NULL DEFAULT 0.00,
    description VARCHAR(255),
    CONSTRAINT chk_debit_credit CHECK (
        (debit >= 0 AND credit = 0) OR 
        (credit >= 0 AND debit = 0)
    )
);

-- Indexing for performance
CREATE INDEX IF NOT EXISTS idx_journal_entries_expense_id ON journal_entries(expense_id);
CREATE INDEX IF NOT EXISTS idx_ledger_lines_journal_entry ON ledger_lines(journal_entry_id);
CREATE INDEX IF NOT EXISTS idx_ledger_lines_account_code ON ledger_lines(account_code);
