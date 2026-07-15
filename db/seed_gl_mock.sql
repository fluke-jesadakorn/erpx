-- Seed GL Mock Data Script
-- Clean up existing journal entries and ledger lines linked to mock data to prevent duplicates
DELETE FROM ledger_lines;
DELETE FROM journal_entries;
DELETE FROM approval_logs;
DELETE FROM expense_items;
DELETE FROM expenses;

-- 1. Insert Mock Expenses

-- Expense 101: Starbucks (Approved, Unpaid Accrued Liability)
INSERT INTO expenses (id, submitter_id, vendor_name, transaction_date, subtotal, vat_amount, total_amount, payment_method, status, is_corrupted, correction_notes, ocr_raw_json)
VALUES (101, 1, 'Starbucks Coffee (Thailand)', '2026-06-24', 299.07, 20.93, 320.00, 'cash', 'approved', false, 'Audited and verified by Accountant.', '{}');

INSERT INTO expense_items (id, expense_id, description, amount, mapped_account_code, confidence_score)
VALUES 
(1011, 101, '1x Cold Brew Coffee Large', 140.00, '510400', 0.98),
(1012, 101, '1x Ham & Cheese Croissant', 180.00, '510400', 0.95);

INSERT INTO approval_logs (expense_id, actor_id, previous_status, new_status, comments)
VALUES 
(101, 1, NULL, 'ocr_extracted', 'Receipt uploaded via LINE OA'),
(101, 3, 'ocr_extracted', 'accountant_reviewed', 'COA codes verified by accountant'),
(101, 2, 'accountant_reviewed', 'approved', 'Authorized by manager');


-- Expense 102: Grab Taxi (Paid and Settled)
INSERT INTO expenses (id, submitter_id, vendor_name, transaction_date, subtotal, vat_amount, total_amount, payment_method, status, is_corrupted, correction_notes, ocr_raw_json)
VALUES (102, 1, 'GrabTaxi (Thailand) Co., Ltd.', '2026-06-24', 450.00, 0.00, 450.00, 'credit_card', 'paid', false, 'Travel allowance approved.', '{}');

INSERT INTO expense_items (id, expense_id, description, amount, mapped_account_code, confidence_score)
VALUES (1021, 102, 'GrabCar Premium service from Head Office to True Digital Park', 450.00, '510200', 0.99);

INSERT INTO approval_logs (expense_id, actor_id, previous_status, new_status, comments)
VALUES 
(102, 1, NULL, 'ocr_extracted', 'Receipt uploaded'),
(102, 3, 'ocr_extracted', 'accountant_reviewed', 'Audited transport cost'),
(102, 2, 'accountant_reviewed', 'approved', 'Approved'),
(102, 3, 'approved', 'paid', 'Reimbursement payment processed.');


-- Expense 103: AWS Hosting (Paid and Settled)
INSERT INTO expenses (id, submitter_id, vendor_name, transaction_date, subtotal, vat_amount, total_amount, payment_method, status, is_corrupted, correction_notes, ocr_raw_json)
VALUES (103, 4, 'Amazon Web Services Inc.', '2026-06-21', 1000.00, 70.00, 1070.00, 'transfer', 'paid', false, 'Monthly cloud subscription billing.', '{}');

INSERT INTO expense_items (id, expense_id, description, amount, mapped_account_code, confidence_score)
VALUES (1031, 103, 'AWS Cloud Infrastructure charges (EC2, RDS Monthly Billing)', 1000.00, '510700', 0.99);

INSERT INTO approval_logs (expense_id, actor_id, previous_status, new_status, comments)
VALUES 
(103, 4, NULL, 'ocr_extracted', 'Receipt uploaded'),
(103, 3, 'ocr_extracted', 'accountant_reviewed', 'Verified software cost'),
(103, 2, 'accountant_reviewed', 'approved', 'Approved budget'),
(103, 3, 'approved', 'paid', 'Reimbursement payment processed.');


-- Expense 104: Office Depot (Math Discrepancy - ocr_extracted)
INSERT INTO expenses (id, submitter_id, vendor_name, transaction_date, subtotal, vat_amount, total_amount, payment_method, status, is_corrupted, correction_notes, ocr_raw_json)
VALUES (104, 1, 'Office Depot (Thailand)', '2026-06-25', 500.00, 35.00, 650.00, 'cash', 'ocr_extracted', true, 'Math Mismatch: Subtotal 500.00 + VAT 35.00 is 535.00, but OCR extracted total amount is 650.00. Needs manual accountant adjustment.', '{}');

INSERT INTO expense_items (id, expense_id, description, amount, mapped_account_code, confidence_score)
VALUES 
(1041, 104, 'A4 Double A Copier Paper 5 Reams', 300.00, '510300', 0.95),
(1042, 104, 'Stationery pack (Notebooks, Pens, Folders)', 200.00, '510300', 0.92);

INSERT INTO approval_logs (expense_id, actor_id, previous_status, new_status, comments)
VALUES (104, 1, NULL, 'ocr_extracted', 'Receipt scanned. Detected mathematical checksum discrepancy.');


-- Expense 105: Adobe Creative Cloud (Audited - accountant_reviewed)
INSERT INTO expenses (id, submitter_id, vendor_name, transaction_date, subtotal, vat_amount, total_amount, payment_method, status, is_corrupted, correction_notes, ocr_raw_json)
VALUES (105, 4, 'Adobe Systems Inc.', '2026-06-25', 1121.50, 78.50, 1200.00, 'credit_card', 'accountant_reviewed', false, 'Accountant reviewed and confirmed software license code.', '{}');

INSERT INTO expense_items (id, expense_id, description, amount, mapped_account_code, confidence_score)
VALUES (1051, 105, 'Adobe Creative Cloud Team Subscription Monthly Billing', 1121.50, '510700', 0.99);

INSERT INTO approval_logs (expense_id, actor_id, previous_status, new_status, comments)
VALUES 
(105, 4, NULL, 'ocr_extracted', 'Receipt uploaded'),
(105, 3, 'ocr_extracted', 'accountant_reviewed', 'Audited and classified to Software & Subscriptions [510700]');


-- Expense 106: Dinner with Client (Rejected by Manager - rejected)
INSERT INTO expenses (id, submitter_id, vendor_name, transaction_date, subtotal, vat_amount, total_amount, payment_method, status, is_corrupted, correction_notes, ocr_raw_json)
VALUES (106, 1, 'Wine Connection Bistro', '2026-06-23', 2336.45, 163.55, 2500.00, 'cash', 'rejected', false, 'Entertainment meal claim.', '{}');

INSERT INTO expense_items (id, expense_id, description, amount, mapped_account_code, confidence_score)
VALUES (1061, 106, 'Dinner with Tech Leads at Wine Connection', 2336.45, '510400', 0.97);

INSERT INTO approval_logs (expense_id, actor_id, previous_status, new_status, comments)
VALUES 
(106, 1, NULL, 'ocr_extracted', 'Receipt uploaded'),
(106, 3, 'ocr_extracted', 'accountant_reviewed', 'Audited client entertainment expense'),
(106, 2, 'accountant_reviewed', 'rejected', 'Over budget limit for entertainment without prior manager authorization.');


-- 2. Insert General Ledger Journal Entries
-- Revenue Journal 1: Monthly Client Billing
INSERT INTO journal_entries (id, expense_id, entry_date, description)
VALUES (2001, NULL, '2026-06-15', 'Monthly service retainer billing - June 2026');

INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
VALUES 
(2001, '110300', 45000.00, 0.00, 'Debit Cash current account - payment received'),
(2001, '410200', 0.00, 45000.00, 'Credit Service Revenue - monthly retainer');


-- Revenue Journal 2: Web Portal Product Sales
INSERT INTO journal_entries (id, expense_id, entry_date, description)
VALUES (2002, NULL, '2026-06-20', 'Product sales revenue - Web portal');

INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
VALUES 
(2002, '110200', 15500.00, 0.00, 'Debit Cash savings account - customer checkout'),
(2002, '410100', 0.00, 15500.00, 'Credit Sales Revenue - digital products');


-- Expense 101 (Starbucks) GL entries: Accrued Liability
INSERT INTO journal_entries (id, expense_id, entry_date, description)
VALUES (2011, 101, '2026-06-24', 'Accrued expense liability from Starbucks Coffee (Thailand) (EXP-101)');

INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
VALUES 
(2011, '510400', 140.00, 0.00, '1x Cold Brew Coffee Large'),
(2011, '510400', 180.00, 0.00, '1x Ham & Cheese Croissant'),
(2011, '110500', 20.93, 0.00, 'Input VAT 7% for EXP-101'),
(2011, '210500', 0.00, 340.93, 'Accrued employee reimbursement payable for EXP-101');


-- Expense 102 (Grab) GL entries: Accrued Liability + Payout Settlement
INSERT INTO journal_entries (id, expense_id, entry_date, description)
VALUES 
(2021, 102, '2026-06-24', 'Accrued expense liability from GrabTaxi (Thailand) Co., Ltd. (EXP-102)'),
(2022, 102, '2026-06-24', 'Settled employee reimbursement for GrabTaxi (Thailand) Co., Ltd. (EXP-102)');

INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
VALUES 
-- Accrual
(2021, '510200', 450.00, 0.00, 'GrabCar Premium service from Head Office to True Digital Park'),
(2021, '210500', 0.00, 450.00, 'Accrued employee reimbursement payable for EXP-102'),
-- Cash Settlement
(2022, '210500', 450.00, 0.00, 'Cleared employee reimbursement payable for EXP-102'),
(2022, '110200', 0.00, 450.00, 'Disbursed cash at bank for EXP-102');


-- Expense 103 (AWS) GL entries: Accrued Liability + Payout Settlement
INSERT INTO journal_entries (id, expense_id, entry_date, description)
VALUES 
(2031, 103, '2026-06-21', 'Accrued expense liability from Amazon Web Services Inc. (EXP-103)'),
(2032, 103, '2026-06-21', 'Settled employee reimbursement for Amazon Web Services Inc. (EXP-103)');

INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
VALUES 
-- Accrual
(2031, '510700', 1000.00, 0.00, 'AWS Cloud Infrastructure charges (EC2, RDS Monthly Billing)'),
(2031, '110500', 70.00, 0.00, 'Input VAT 7% for EXP-103'),
(2031, '210500', 0.00, 1070.00, 'Accrued employee reimbursement payable for EXP-103'),
-- Cash Settlement
(2032, '210500', 1070.00, 0.00, 'Cleared employee reimbursement payable for EXP-103'),
(2032, '110200', 0.00, 1070.00, 'Disbursed cash at bank for EXP-103');

-- Reset sequences for postgres IDs so new records continue correctly
SELECT setval('expenses_id_seq', COALESCE((SELECT MAX(id)+1 FROM expenses), 1), false);
SELECT setval('expense_items_id_seq', COALESCE((SELECT MAX(id)+1 FROM expense_items), 1), false);
SELECT setval('journal_entries_id_seq', COALESCE((SELECT MAX(id)+1 FROM journal_entries), 1), false);
SELECT setval('ledger_lines_id_seq', COALESCE((SELECT MAX(id)+1 FROM ledger_lines), 1), false);
SELECT setval('approval_logs_id_seq', COALESCE((SELECT MAX(id)+1 FROM approval_logs), 1), false);
