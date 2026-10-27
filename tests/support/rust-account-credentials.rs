use erpx_server_identity_access::public::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{Clock, TimestampMillis};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
#[path = "rust-account.rs"]
pub mod account;
pub const PASSWORD: &str = "synthetic-fixture-password";
pub const NEW_PASSWORD: &str = "different-synthetic-password";
pub struct MutableClock(pub AtomicI64);
impl Clock for MutableClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0.load(Ordering::SeqCst))
    }
}
impl MutableClock {
    pub fn advance(&self, millis: i64) {
        self.0.fetch_add(millis, Ordering::SeqCst);
    }
}
pub struct Scenario {
    pub auth: Arc<AccountAuthentication>,
    pub credentials: Arc<AccountCredentials>,
    pub keyring: Arc<AccountMailKeyring>,
    pub clock: Arc<MutableClock>,
}
pub async fn setup(
    pool: &DatabasePool,
    invitations: Arc<dyn InvitationCredentialAuthority>,
) -> Scenario {
    setup_with_clock(
        pool,
        invitations,
        Arc::new(MutableClock(AtomicI64::new(
            account::FixedClock.now().get(),
        ))),
    )
    .await
}
pub async fn setup_with_clock(
    pool: &DatabasePool,
    invitations: Arc<dyn InvitationCredentialAuthority>,
    clock: Arc<MutableClock>,
) -> Scenario {
    setup_with_clock_and_invalidation(
        pool,
        invitations,
        clock,
        Arc::new(account::RecordedFixtureRevocations::default()),
    )
    .await
}
pub async fn setup_with_clock_and_invalidation(
    pool: &DatabasePool,
    invitations: Arc<dyn InvitationCredentialAuthority>,
    clock: Arc<MutableClock>,
    invalidation: Arc<dyn account_revocation::AccountWorkContextInvalidation>,
) -> Scenario {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::Control,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::Control, &files)
        .await
        .unwrap();
    sqlx::query("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding) VALUES('h001','acme','Synthetic Acme','HOLDING_H001_DB','HOLDING_H001_READ_DB','HOLDING_H001_FILES')").execute(pool.raw()).await.unwrap();
    let passwords = Arc::new(Argon2Passwords::new(2).unwrap());
    let hash = passwords
        .hash(Password::new_credential(PASSWORD.into()).unwrap())
        .await
        .unwrap();
    for (id, email, verified) in [
        ("fixture-a", "a@example.test", 1_i64),
        ("fixture-b", "b@example.test", 1),
        ("fixture-unverified", "unverified@example.test", 0),
    ] {
        sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") VALUES($1,$1,$2,$3,$4,$4)").bind(id).bind(email).bind(verified).bind(account::NOW).execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\") VALUES($1,$1,'credential','local:credential',$1,$2,$3,$3)").bind(id).bind(hash.as_str()).bind(account::NOW).execute(pool.raw()).await.unwrap();
    }
    let keyring = Arc::new(
        AccountMailKeyring::new(
            "synthetic-test-only".into(),
            vec![("synthetic-test-only".into(), [7; 32])],
        )
        .unwrap(),
    );
    let secrets = Arc::new(SystemAuthenticationSecrets);
    let auth = Arc::new(
        AccountAuthentication::new(
            account::policy(),
            Arc::new(
                PostgresAccountRepository::from_control_database(
                    pool.clone(),
                    "development".into(),
                    true,
                    invalidation.clone(),
                    clock.clone(),
                )
                .unwrap(),
            ),
            passwords.clone(),
            secrets.clone(),
            clock.clone(),
        )
        .unwrap(),
    );
    let credentials = Arc::new(
        AccountCredentials::new(
            account::policy(),
            Arc::new(
                PostgresAccountCredentials::from_control_database(
                    pool.clone(),
                    "development".into(),
                    invitations,
                    invalidation.clone(),
                    clock.clone(),
                )
                .unwrap(),
            ),
            passwords,
            secrets,
            keyring.clone(),
            clock.clone(),
        )
        .unwrap(),
    );
    Scenario {
        auth,
        credentials,
        keyring,
        clock,
    }
}
pub async fn mail(
    pool: &DatabasePool,
    scenario: &Scenario,
    kind: CredentialProofKind,
    email: &str,
) -> OpenedAccountMail {
    let (id,cipher):(String,String)=sqlx::query_as("SELECT p.id,o.payload_ciphertext FROM native_account_mail_outbox o JOIN native_account_proofs p ON p.id=o.proof_id WHERE p.kind=$1 AND p.email=$2 ORDER BY p.created_at DESC,p.id DESC LIMIT 1").bind(kind.as_str()).bind(email).fetch_one(pool.raw()).await.unwrap();
    scenario.keyring.open(&id, &cipher).unwrap()
}
pub fn remember(signed: SignedIn) -> PresentedSecrets {
    PresentedSecrets {
        browser: signed.browser_secret,
        active: Some(signed.session_secret.clone()),
        saved: vec![(signed.handle, signed.session_secret)],
    }
}
