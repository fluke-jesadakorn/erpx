-- Sensitive sandbox deliveries retain only an authenticated encrypted envelope.
ALTER TABLE sandbox_email_deliveries
  ADD COLUMN body_ciphertext TEXT;

-- Older sandbox verification receipts contain bearer links in body_text. They
-- can no longer be recovered from the sandbox; users can request a fresh link.
UPDATE sandbox_email_deliveries
SET body_text = '[redacted sensitive email body]'
WHERE body_ciphertext IS NULL
  AND subject LIKE 'Verify your % email';

ALTER TABLE sandbox_email_deliveries
  ADD CONSTRAINT sandbox_email_deliveries_ciphertext_redacted
  CHECK (
    body_ciphertext IS NULL OR
    (body_ciphertext <> '' AND body_text = '[redacted sensitive email body]')
  );
