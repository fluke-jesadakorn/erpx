-- A general support report is triaged before it has an owner or follow-up date.
-- Legacy sales-linked tickets retain their source, customer and confirmation rules.
ALTER TABLE support_tickets
  ALTER COLUMN branch_id DROP NOT NULL,
  ALTER COLUMN order_id DROP NOT NULL,
  ALTER COLUMN customer_id DROP NOT NULL,
  ALTER COLUMN owner_id DROP NOT NULL,
  ALTER COLUMN due_date DROP NOT NULL,
  ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'sales_order',
  ADD COLUMN confirmed_by TEXT,
  ADD COLUMN confirmed_at BIGINT,
  ADD COLUMN confirmation_reason TEXT;

ALTER TABLE support_ticket_events
  ALTER COLUMN owner_id DROP NOT NULL,
  ALTER COLUMN due_date DROP NOT NULL;

DO $$ DECLARE constraint_name TEXT; BEGIN
  FOR constraint_name IN
    SELECT conname FROM pg_constraint
    WHERE conrelid = 'support_tickets'::regclass AND contype = 'c'
      AND pg_get_constraintdef(oid) ILIKE '%status%'
  LOOP
    EXECUTE format('ALTER TABLE support_tickets DROP CONSTRAINT %I', constraint_name);
  END LOOP;
END $$;

ALTER TABLE support_tickets
  ADD CONSTRAINT support_ticket_source_kind CHECK (
    (source_kind = 'sales_order' AND branch_id IS NOT NULL AND order_id IS NOT NULL AND customer_id IS NOT NULL)
    OR (source_kind = 'general' AND order_id IS NULL AND customer_id IS NULL)
  ),
  ADD CONSTRAINT support_ticket_status CHECK (status IN ('triage','open','in_progress','waiting_customer','resolved','closed','cancelled') AND (status <> 'triage' OR source_kind = 'general')),
  ADD CONSTRAINT support_ticket_assignment CHECK (
    (status = 'triage' AND owner_id IS NULL AND due_date IS NULL)
    OR (status = 'cancelled' AND owner_id IS NULL AND due_date IS NULL)
    OR (status NOT IN ('triage','cancelled') AND owner_id IS NOT NULL AND due_date IS NOT NULL)
    OR (status = 'cancelled' AND owner_id IS NOT NULL AND due_date IS NOT NULL)
  ),
  ADD CONSTRAINT support_ticket_resolution CHECK (status <> 'resolved' OR resolution IS NOT NULL),
  ADD CONSTRAINT support_ticket_closure CHECK (status <> 'closed' OR (
    resolution IS NOT NULL AND (
      (source_kind = 'sales_order' AND confirmation_note_id IS NOT NULL AND confirmation_note_version IS NOT NULL)
      OR (source_kind = 'general' AND confirmed_by IS NOT NULL AND confirmed_at IS NOT NULL AND confirmation_reason IS NOT NULL)
    )
  ));

CREATE FUNCTION support_ticket_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
  IF (NEW.company_id,NEW.branch_id,NEW.department_id,NEW.order_id,NEW.customer_id,NEW.source_kind,NEW.created_by)
     IS DISTINCT FROM
     (OLD.company_id,OLD.branch_id,OLD.department_id,OLD.order_id,OLD.customer_id,OLD.source_kind,OLD.created_by)
  THEN RAISE EXCEPTION 'SUPPORT_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER support_ticket_source_guard BEFORE UPDATE ON support_tickets
  FOR EACH ROW EXECUTE FUNCTION support_ticket_source_immutable();
