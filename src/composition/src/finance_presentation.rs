//! Finite read-only Finance UI decisions from Commercial's actual owner port.
//! The caller retains its own current actor/document unit; these are no grants.
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseInspection, PurchaseRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde::Serialize;
use sqlx::PgConnection;
use std::collections::BTreeMap;
type Error = PgTransactionError<BusinessError>;
const CAPABILITIES: [&str; 7] = [
    "finance.accounting",
    "finance.banking",
    "finance.close",
    "finance.advanced",
    "finance.receivables",
    "finance.tax",
    "finance.service-invoice",
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FinancePurchaseAccess {
    pub new_work: bool,
    pub settle_existing: bool,
    pub history: bool,
    pub revision: String,
}
pub struct CurrentFinancePresentation {
    values: BTreeMap<String, FinancePurchaseAccess>,
    observations: Vec<PurchaseInspection>,
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "FINANCE_PRESENTATION_CHANGED",
        "The current Finance access changed. Reload the document.",
    )
}
impl CurrentFinancePresentation {
    pub fn values(&self) -> &BTreeMap<String, FinancePurchaseAccess> {
        &self.values
    }
    pub fn service_purchase(&self) -> serde_json::Value {
        let a = &self.values["finance.service-invoice"];
        serde_json::json!({"capability":"finance.service-invoice","settleExisting":a.settle_existing,"history":a.history,"revision":a.revision})
    }
    pub fn can_post_invoice(&self) -> bool {
        self.values["finance.accounting"].settle_existing
            || self.values["finance.service-invoice"].settle_existing
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        holding: &HoldingId,
        company: &CompanyId,
        catalogue: &CommercialCatalogue,
        clock: &impl Clock,
    ) -> Result<(), Error> {
        let mut reader =
            PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue);
        for before in &self.observations {
            let after = reader.revalidate(&before.witness, clock.now()).await?;
            if before.revision != after.revision || before.access != after.access {
                return Err(changed().into());
            }
        }
        Ok(())
    }
}
pub async fn read_finance_presentation(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    clock: &impl Clock,
) -> Result<CurrentFinancePresentation, Error> {
    let mut reader = PgCompanyPurchaseReader::new(holding.clone(), company.clone(), c, catalogue);
    let mut observations = Vec::new();
    for purpose in [
        CapabilityPurpose::NewWork,
        CapabilityPurpose::SettleExisting,
        CapabilityPurpose::History,
    ] {
        observations.push(
            reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: CAPABILITIES.iter().map(|v| (*v).into()).collect(),
                        purpose,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    clock.now(),
                )
                .await?,
        );
    }
    let allowed = |index: usize, cap: &str| -> Result<bool, BusinessError> {
        let result = &observations[index];
        let access = result
            .access
            .iter()
            .find(|a| a.capability == cap)
            .ok_or_else(changed)?;
        Ok(access.allowed_by_mode)
    };
    let mut values = BTreeMap::new();
    for capability in CAPABILITIES {
        values.insert(
            capability.into(),
            FinancePurchaseAccess {
                new_work: allowed(0, capability)?,
                settle_existing: allowed(1, capability)?,
                history: allowed(2, capability)?,
                revision: observations[2].revision.clone(),
            },
        );
    }
    Ok(CurrentFinancePresentation {
        values,
        observations,
    })
}
