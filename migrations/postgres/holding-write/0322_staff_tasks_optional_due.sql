-- Ordinary staff tasks can be created without a deadline. Existing due dates and
-- routine schedule deadlines remain unchanged.
ALTER TABLE staff_tasks ALTER COLUMN due_at DROP NOT NULL;
