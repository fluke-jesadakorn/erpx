#[derive(Default)]
pub struct JournalFacts {
    pub deny: bool,
    pub no_chart: bool,
    pub unsupported_scale: bool,
    pub wrong_accounts: bool,
    pub opening: bool,
    pub calls: Vec<&'static str>,
}

use erpx_server_finance::public::{
    FinanceActor, FunctionalFxEvidence, JournalChart, JournalDraft, JournalError,
    JournalPreparationPorts,
};
use erpx_server_shared::public::{CompanyId, FailureKind, TimestampMillis};

impl JournalFacts {
    fn scoped(&self, company: &CompanyId) {
        assert_eq!(company.as_str(), "company");
    }
}

impl JournalPreparationPorts for JournalFacts {
    type Error = JournalError;
    async fn transaction_witness(
        &mut self,
    ) -> Result<Option<erpx_server_finance::public::JournalTransactionWitness>, Self::Error> {
        self.calls.push("transaction");
        Ok(None)
    }
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        _draft: &JournalDraft,
    ) -> Result<(), JournalError> {
        self.scoped(&actor.company_id);
        self.calls.push("authorize");
        if self.deny {
            Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "FINANCE_FORBIDDEN",
                message: "Denied".to_owned(),
            })
        } else {
            Ok(())
        }
    }
    async fn chart(
        &mut self,
        company: &CompanyId,
        _chart: &str,
    ) -> Result<Option<JournalChart>, JournalError> {
        self.scoped(company);
        self.calls.push("chart");
        Ok((!self.no_chart).then(|| JournalChart {
            id: "chart".to_owned(),
            base_currency: "USD".to_owned(),
        }))
    }
    async fn ensure_period(
        &mut self,
        actor: &FinanceActor,
        _period: &str,
        _date: &str,
    ) -> Result<(), JournalError> {
        self.scoped(&actor.company_id);
        self.calls.push("period");
        Ok(())
    }
    async fn money_scale(
        &mut self,
        company: &CompanyId,
        _currency: &str,
    ) -> Result<u8, JournalError> {
        self.scoped(company);
        self.calls.push("scale");
        Ok(if self.unsupported_scale { 3 } else { 2 })
    }
    async fn functional_fx(
        &mut self,
        actor: &FinanceActor,
        _currency: &str,
        _base: &str,
        _date: &str,
    ) -> Result<FunctionalFxEvidence, JournalError> {
        self.scoped(&actor.company_id);
        self.calls.push("fx");
        Ok(FunctionalFxEvidence {
            micros: 1_000_000,
            source_rate_id: None,
            target_rate_id: None,
            used_latest_fallback: false,
        })
    }
    async fn accounts(
        &mut self,
        company: &CompanyId,
        _chart: &str,
        ids: &[String],
    ) -> Result<Vec<String>, JournalError> {
        self.scoped(company);
        self.calls.push("accounts");
        Ok(if self.wrong_accounts {
            vec!["other".to_owned(); ids.len()]
        } else {
            ids.to_vec()
        })
    }
    async fn opening_exists(
        &mut self,
        company: &CompanyId,
        _chart: &str,
    ) -> Result<bool, JournalError> {
        self.scoped(company);
        self.calls.push("opening");
        Ok(self.opening)
    }
    fn now(&mut self) -> TimestampMillis {
        self.calls.push("clock");
        erpx_server_shared::public::Clock::now(&FixedClock)
    }
}

pub struct FixedClock;
impl erpx_server_shared::public::Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(100)
    }
}

// Test-only authority; production composition has no permissive implementation.
impl erpx_server_finance::public::JournalAuthority for JournalFacts {
    type Error = erpx_server_runtime::public::postgres::PgTransactionError<JournalError>;
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        JournalPreparationPorts::authorize(self, actor, draft)
            .await
            .map_err(erpx_server_runtime::public::postgres::PgTransactionError::Business)
    }
}
