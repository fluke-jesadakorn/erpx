//! Fixed private collation profile corresponding to the qualified source runtime.
use super::payroll_accounting_operand::{Fault, R};
use icu_collator::{
    Collator, CollatorBorrowed, CollatorPreferences,
    options::{AlternateHandling, CaseLevel, CollatorOptions, Strength},
    preferences::{CollationCaseFirst, CollationNumericOrdering},
};
use icu_locale::Locale;
pub(crate) fn collator() -> R<CollatorBorrowed<'static>> {
    let locale: Locale = "en-US"
        .parse()
        .map_err(|_| Fault::gap("P3", "collation", "locale"))?;
    let mut preferences: CollatorPreferences = locale.into();
    preferences.case_first = Some(CollationCaseFirst::False);
    preferences.numeric_ordering = Some(CollationNumericOrdering::False);
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    options.alternate_handling = Some(AlternateHandling::NonIgnorable);
    options.case_level = Some(CaseLevel::Off);
    Collator::try_new(preferences, options).map_err(|_| Fault::gap("P3", "collation", "provider"))
}
