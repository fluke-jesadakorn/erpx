//! The retained production Service-first-Count module conjunction.
//! These facts are not Stock, purchase, or personal authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialCountModuleMode {
    Development,
    Production,
}
/// `field_service` is registered but absent from the release scope. Thus the
/// three retained checks all pass in production exactly when the sealed
/// isolated-engineering predicate succeeds; the other two rollouts do not
/// bypass that first check.
pub fn initial_count_modules_allowed(mode: InitialCountModuleMode, engineering: bool) -> bool {
    match mode {
        InitialCountModuleMode::Development => true,
        InitialCountModuleMode::Production => engineering,
    }
}
