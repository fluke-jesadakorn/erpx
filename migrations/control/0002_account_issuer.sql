ALTER TABLE account ADD COLUMN issuer TEXT NOT NULL DEFAULT 'local:credential';

CREATE UNIQUE INDEX IF NOT EXISTS account_issuer_idx
  ON account(issuer, accountId);
