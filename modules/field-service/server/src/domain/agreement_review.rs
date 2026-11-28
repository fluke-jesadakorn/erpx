//! Pure state transitions for the existing Agreement review command.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgreementReviewAction {
    Activate,
    Reject,
    Retire,
}

impl AgreementReviewAction {
    pub fn operation(self) -> &'static str {
        match self {
            Self::Activate => "field.agreement.activate",
            Self::Reject => "field.agreement.reject",
            Self::Retire => "field.agreement.retire",
        }
    }

    pub fn expected_status(self) -> &'static str {
        match self {
            Self::Activate | Self::Reject => "draft",
            Self::Retire => "active",
        }
    }

    pub fn status(self) -> &'static str {
        match self {
            Self::Activate => "active",
            Self::Reject => "rejected",
            Self::Retire => "retired",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AgreementReviewAction;

    #[test]
    fn retained_review_transitions() {
        for (action, operation, expected, status) in [
            (
                AgreementReviewAction::Activate,
                "field.agreement.activate",
                "draft",
                "active",
            ),
            (
                AgreementReviewAction::Reject,
                "field.agreement.reject",
                "draft",
                "rejected",
            ),
            (
                AgreementReviewAction::Retire,
                "field.agreement.retire",
                "active",
                "retired",
            ),
        ] {
            assert_eq!(action.operation(), operation);
            assert_eq!(action.expected_status(), expected);
            assert_eq!(action.status(), status);
        }
    }
}
