-- Better Auth 1.7.3+ identifies accounts by (providerId, accountId).
-- Preserve historical issuer values without constraining new provider accounts.
-- The existing account_provider_idx continues enforcing the provider identity.
DROP INDEX IF EXISTS account_issuer_idx;
ALTER TABLE account ALTER COLUMN issuer DROP NOT NULL;
ALTER TABLE account ALTER COLUMN issuer DROP DEFAULT;
