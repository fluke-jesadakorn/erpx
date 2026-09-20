CREATE TABLE pos_stripe_routes(account_id TEXT NOT NULL,livemode BOOLEAN NOT NULL,holding_id TEXT NOT NULL,company_id TEXT NOT NULL,created_at BIGINT NOT NULL,PRIMARY KEY(account_id,livemode));
