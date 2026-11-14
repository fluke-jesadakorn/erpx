/// Chosen only by trusted bootstrap code, never an HTTP or serialized grant.
#[derive(Clone, Copy, Debug)]
pub enum FinanceJournalEnvironment {
    Development,
    Production,
}
