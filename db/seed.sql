-- Seed Roles
INSERT INTO roles (id, name) VALUES
(1, 'staff'),
(2, 'accountant'),
(3, 'manager'),
(4, 'admin')
ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name;

-- Seed Users
INSERT INTO users (employee_code, fullname, role_id, department) VALUES
('EMP001', 'John Staff', 1, 'Development'),
('EMP002', 'Sarah Approver', 3, 'Engineering'),
('EMP003', 'Mark Reviewer', 2, 'Finance & Account'),
('EMP004', 'Emily Manager', 1, 'Human Resource'),
('EMP005', 'Olivia Director', 4, 'Executive')
ON CONFLICT (employee_code) DO NOTHING;

-- Seed Expanded Chart of Accounts (COA)
INSERT INTO chart_of_accounts (code, name, name_th, account_type, embedding) VALUES
-- 1. ASSETS
('110100', 'Cash on Hand', 'Cash on Hand', 'asset', NULL),
('110200', 'Cash at Bank - Savings', 'Savings Deposits', 'asset', NULL),
('110300', 'Cash at Bank - Current', 'Current Account Deposits', 'asset', NULL),
('110400', 'Accounts Receivable', 'Accounts Receivable', 'asset', NULL),
('110500', 'Input VAT', 'Input VAT', 'asset', NULL),
('110600', 'Prepaid Expenses', 'Prepaid Expenses', 'asset', NULL),

-- 2. LIABILITIES
('210100', 'Accounts Payable', 'Accounts Payable', 'liability', NULL),
('210200', 'Accrued Expenses', 'Accrued Expenses', 'liability', NULL),
('210300', 'Accrued Output VAT', 'Accrued Output VAT', 'liability', NULL),
('210400', 'Withholding Tax Payable', 'Withholding Tax Payable', 'liability', NULL),
('210500', 'Employee Reimbursement Payable', 'Employee Reimbursement Payable', 'liability', NULL),

-- 3. EQUITY
('310100', 'Share Capital', 'Share Capital', 'equity', NULL),
('310200', 'Retained Earnings', 'Retained Earnings', 'equity', NULL),

-- 4. REVENUES
('410100', 'Sales Revenue', 'Sales Revenue', 'revenue', NULL),
('410200', 'Service Revenue', 'Service Revenue', 'revenue', NULL),
('410300', 'Other Income', 'Other Income', 'revenue', NULL),

-- 5. EXPENSES
('510100', 'Salaries & Wages', 'Salaries and Wages', 'expense', NULL),
('510200', 'Travel & Transportation', 'Travel and Transportation Expenses', 'expense', NULL),
('510300', 'Office Supplies & Stationery', 'Office Stationery and Supplies', 'expense', NULL),
('510400', 'Entertainment & Client Meal', 'Client Entertainment and Meals', 'expense', NULL),
('510500', 'Internet & Utilities', 'Internet and Utilities', 'expense', NULL),
('510600', 'Post & Delivery', 'Postage and Delivery', 'expense', NULL),
('510700', 'Software & Subscriptions', 'Software and Cloud Services', 'expense', NULL),
('510800', 'Maintenance & Repairs', 'Repairs and Maintenance', 'expense', NULL),
('510900', 'Training & Seminar', 'Training and Seminars', 'expense', NULL),
('520100', 'Office Rental', 'Office Rental', 'expense', NULL),
('520200', 'Marketing & Advertising', 'Marketing and Advertising', 'expense', NULL),
('520300', 'Professional & Consulting Fees', 'Professional and Consulting Fees', 'expense', NULL),
('520400', 'Bank Charges', 'Bank Charges', 'expense', NULL),
('520500', 'Insurance Expenses', 'Insurance Premiums', 'expense', NULL),
('520600', 'Depreciations', 'Depreciation', 'expense', NULL),
('520700', 'Taxes & Licenses', 'Taxes and License Fees', 'expense', NULL)
ON CONFLICT (code) DO UPDATE SET 
    name = EXCLUDED.name,
    name_th = EXCLUDED.name_th,
    account_type = EXCLUDED.account_type;
