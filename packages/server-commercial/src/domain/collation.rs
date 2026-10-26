use erpx_server_shared::public::{BusinessError, FailureKind};
use icu_collator::{
    Collator, CollatorBorrowed, CollatorPreferences,
    options::{AlternateHandling, CaseLevel, CollatorOptions, Strength},
    preferences::{CollationCaseFirst, CollationNumericOrdering},
};
use icu_locale::Locale;

/// Match the retained Node 26.11.1 `String.localeCompare` sort contract.
///
/// That runtime uses en-US, ICU 78.3 / CLDR 48 / Unicode 17. Compiled ICU4X
/// collation and data are pinned to 2.1.0 (CLDR 48); no host locale, I/O or
/// process-global provider participates. Callers must use a stable sort and
/// preserve `Equal` for canonically equivalent, byte-distinct persisted IDs.
/// An ordinal tie-breaker or Unicode normalization would change receipt hashes.
pub(crate) fn package_id_collator() -> Result<CollatorBorrowed<'static>, BusinessError> {
    let locale: Locale = "en-US".parse().map_err(|_| unavailable())?;
    let mut preferences: CollatorPreferences = locale.into();
    preferences.case_first = Some(CollationCaseFirst::False);
    preferences.numeric_ordering = Some(CollationNumericOrdering::False);
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    options.alternate_handling = Some(AlternateHandling::NonIgnorable);
    options.case_level = Some(CaseLevel::Off);
    Collator::try_new(preferences, options).map_err(|_| unavailable())
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCIAL_COLLATION_UNAVAILABLE",
        "Package collation data is unavailable.",
    )
}
