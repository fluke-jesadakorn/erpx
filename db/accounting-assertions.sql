BEGIN;

DO $assert$
DECLARE
  branch bigint;
  posting date;
  journal bigint;
  posted bigint;
  reversal bigint;
BEGIN
  SELECT id INTO branch FROM finance.branches WHERE code = 'HQ';
  SELECT starts_on INTO posting FROM finance.fiscal_periods WHERE status = 'open' ORDER BY starts_on LIMIT 1;

  INSERT INTO finance.journals(posting_date, document_date, description, source_type, source_id, source_event_key, branch_id, created_by)
  VALUES (posting, posting, 'Zero-sided rejection', 'assertion', 'zero', 'assertion:zero', branch, 1)
  RETURNING id INTO journal;
  BEGIN
    INSERT INTO finance.journal_lines(journal_id, line_no, account_code, description, debit_thb, credit_thb, branch_id)
    VALUES (journal, 1, '110100', 'Invalid zero line', 0, 0, branch);
    RAISE EXCEPTION 'Zero-sided line was accepted';
  EXCEPTION WHEN check_violation THEN NULL;
  END;

  BEGIN
    INSERT INTO finance.journal_lines(journal_id, line_no, account_code, description, debit_thb, credit_thb, branch_id)
    VALUES (journal, 1, 'INVALID', 'Invalid account', 1, 0, branch);
    RAISE EXCEPTION 'Invalid account was accepted';
  EXCEPTION WHEN foreign_key_violation THEN NULL;
  END;

  INSERT INTO finance.journal_lines(journal_id, line_no, account_code, description, debit_thb, credit_thb, branch_id)
  VALUES (journal, 1, '110100', 'Unbalanced debit', 100, 0, branch);
  UPDATE finance.journals SET status = 'prepared', preparer_id = 1, prepared_at = now() WHERE id = journal;
  BEGIN
    PERFORM finance.post_journal(journal, 1);
    RAISE EXCEPTION 'Unbalanced journal was posted';
  EXCEPTION WHEN raise_exception THEN
    IF SQLERRM = 'Unbalanced journal was posted' THEN RAISE; END IF;
  END;

  BEGIN
    INSERT INTO finance.journals(posting_date, document_date, description, source_type, source_id, source_event_key, branch_id, created_by)
    VALUES (posting, posting, 'Duplicate source', 'assertion', 'duplicate', 'assertion:zero', branch, 1);
    RAISE EXCEPTION 'Duplicate source event was accepted';
  EXCEPTION WHEN unique_violation THEN NULL;
  END;

  INSERT INTO finance.journals(posting_date, document_date, description, source_type, source_id, source_event_key, branch_id, status, preparer_id, prepared_at, created_by)
  VALUES (posting, posting, 'Locked period rejection', 'assertion', 'locked', 'assertion:locked', branch, 'prepared', 1, now(), 1)
  RETURNING id INTO journal;
  INSERT INTO finance.journal_lines(journal_id, line_no, account_code, description, debit_thb, credit_thb, branch_id) VALUES
    (journal, 1, '110100', 'Debit', 100, 0, branch),
    (journal, 2, '410100', 'Credit', 0, 100, branch);
  UPDATE finance.fiscal_periods SET status = 'locked' WHERE posting BETWEEN starts_on AND ends_on;
  BEGIN
    PERFORM finance.post_journal(journal, 1);
    RAISE EXCEPTION 'Locked-period journal was posted';
  EXCEPTION WHEN raise_exception THEN
    IF SQLERRM = 'Locked-period journal was posted' THEN RAISE; END IF;
  END;
  UPDATE finance.fiscal_periods SET status = 'open' WHERE posting BETWEEN starts_on AND ends_on;

  INSERT INTO finance.journals(posting_date, document_date, description, source_type, source_id, source_event_key, branch_id, status, preparer_id, prepared_at, created_by)
  VALUES (posting, posting, 'Posted immutability', 'assertion', 'posted', 'assertion:posted', branch, 'prepared', 1, now(), 1)
  RETURNING id INTO posted;
  INSERT INTO finance.journal_lines(journal_id, line_no, account_code, description, debit_thb, credit_thb, branch_id) VALUES
    (posted, 1, '110100', 'Debit', 250, 0, branch),
    (posted, 2, '410100', 'Credit', 0, 250, branch);
  PERFORM finance.post_journal(posted, 1);
  BEGIN
    UPDATE finance.journals SET description = 'Mutated' WHERE id = posted;
    RAISE EXCEPTION 'Posted journal was mutable';
  EXCEPTION WHEN raise_exception THEN
    IF SQLERRM = 'Posted journal was mutable' THEN RAISE; END IF;
  END;
  BEGIN
    DELETE FROM finance.journal_lines WHERE journal_id = posted AND line_no = 1;
    RAISE EXCEPTION 'Posted journal line was mutable';
  EXCEPTION WHEN raise_exception THEN
    IF SQLERRM = 'Posted journal line was mutable' THEN RAISE; END IF;
  END;

  INSERT INTO finance.journals(posting_date, document_date, description, source_type, source_id, source_event_key, branch_id, status, preparer_id, prepared_at, reversal_of_id, created_by)
  VALUES (posting, posting, 'Reversal', 'assertion', 'reversal', 'assertion:reversal', branch, 'prepared', 1, now(), posted, 1)
  RETURNING id INTO reversal;
  INSERT INTO finance.journal_lines(journal_id, line_no, account_code, description, debit_thb, credit_thb, branch_id) VALUES
    (reversal, 1, '410100', 'Reverse credit', 250, 0, branch),
    (reversal, 2, '110100', 'Reverse debit', 0, 250, branch);
  PERFORM finance.post_journal(reversal, 1);
  IF NOT EXISTS (
    SELECT 1 FROM finance.journals r
    WHERE r.id = reversal AND r.status = 'posted' AND r.reversal_of_id = posted
  ) THEN
    RAISE EXCEPTION 'Linked reversal was not posted';
  END IF;

  INSERT INTO inventory.products(sku, name, base_unit)
  VALUES ('ASSERTION-SKU', 'Assertion product', 'EA');
  BEGIN
    INSERT INTO inventory.stock_balances(product_id, warehouse_id, quantity, avg_cost_thb)
    SELECT p.id, w.id, -1, 0
      FROM inventory.products p
     CROSS JOIN inventory.warehouses w
     WHERE p.sku = 'ASSERTION-SKU'
     LIMIT 1;
    RAISE EXCEPTION 'Negative stock was accepted';
  EXCEPTION WHEN check_violation THEN NULL;
  END;
END
$assert$;

ROLLBACK;
