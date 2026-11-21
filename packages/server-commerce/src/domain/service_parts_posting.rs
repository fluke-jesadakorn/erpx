//! Decisions over already-authorized and owner-verified source facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostingMode {
    Initialize,
    ConfirmExisting,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostingAdmission {
    Allowed,
    AlreadyConfigured,
    MappingRequired,
    HistoryRequiresReview,
    ExistingMappingRequiresConfirmation,
}
/// Finance references, purchase and current actor checks are separate owner proofs.
pub fn posting_admission(
    mode: PostingMode,
    has_mapping: bool,
    has_source: bool,
    has_history: bool,
) -> PostingAdmission {
    if has_source {
        return PostingAdmission::AlreadyConfigured;
    }
    match mode {
        PostingMode::ConfirmExisting if !has_mapping => PostingAdmission::MappingRequired,
        PostingMode::Initialize if has_mapping => {
            PostingAdmission::ExistingMappingRequiresConfirmation
        }
        PostingMode::Initialize if has_history => PostingAdmission::HistoryRequiresReview,
        _ => PostingAdmission::Allowed,
    }
}
