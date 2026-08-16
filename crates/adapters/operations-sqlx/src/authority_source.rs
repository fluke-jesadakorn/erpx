//! Normalized persistence for the closed Operations `SourceFact` union.

use serde::{Serialize, de::DeserializeOwned};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use erpx_cqrs_sqlx::CqrsSqlxError;
use erpx_integration_contracts::{
    BoundedContext, CurrencyCode, DisplayName, MoneyAmount, Quantity, RecordRef, ReferenceCode,
    assets, commercial, crm, customer, finance, inventory, legal, maintenance, marketing,
    operations, people_payroll, pos, procurement, quality, sales, support,
};
use erpx_operations::{ExceptionId, PortError};

const SOURCE_ROOTS: [&str; 18] = [
    "source_commercial_authority",
    "source_customer_authority",
    "source_marketing_authority",
    "source_marketing_approval_authority",
    "source_crm_authority",
    "source_procurement_authority",
    "source_inventory_authority",
    "source_assets_authority",
    "source_maintenance_authority",
    "source_quality_authority",
    "source_sales_authority",
    "source_people_payroll_authority",
    "source_pos_sale_authority",
    "source_pos_refund_authority",
    "source_finance_authority",
    "source_legal_authority",
    "source_support_authority",
    "source_support_incident_authority",
];

pub(crate) async fn load_source_fact(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
) -> Result<operations::SourceFact, PortError> {
    let kind: String = sqlx::query_scalar(
        "SELECT fact_kind FROM erpx_operations_write.exception_source_fact_authority
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND exception_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?
    .ok_or(PortError::Internal)?;
    let fact = match kind.as_str() {
        "commercial" => {
            let row = variant_row(tx, "source_commercial_authority", id).await?;
            operations::SourceFact::Commercial(commercial::SubscriptionFact {
                subscription: commercial::SubscriptionRef::try_from_uuid(uuid(
                    &row,
                    "subscription_id",
                )?)
                .map_err(|_| PortError::Internal)?,
                customer: customer::CustomerRef::try_from_uuid(uuid(&row, "customer_id")?)
                    .map_err(|_| PortError::Internal)?,
                plan: commercial::PlanRef::try_from_uuid(uuid(&row, "plan_id")?)
                    .map_err(|_| PortError::Internal)?,
                status: enum_value(text(&row, "subscription_status")?)?,
                recurring_amount: money(&row, "recurring_currency", "recurring_minor")?,
                effective_at: row
                    .try_get("effective_at")
                    .map_err(|_| PortError::Internal)?,
            })
        }
        "customer" => {
            let row = variant_row(tx, "source_customer_authority", id).await?;
            operations::SourceFact::Customer(customer::CustomerProfileFact {
                customer: customer::CustomerRef::try_from_uuid(uuid(&row, "customer_id")?)
                    .map_err(|_| PortError::Internal)?,
                account: customer::CustomerAccountRef::try_from_uuid(uuid(&row, "account_id")?)
                    .map_err(|_| PortError::Internal)?,
                customer_code: code(&row, "customer_code")?,
                display_name: display(&row, "display_name")?,
                owning_company:
                    erpx_integration_contracts::organization::CompanyRef::try_from_uuid(uuid(
                        &row,
                        "owning_company_id",
                    )?)
                    .map_err(|_| PortError::Internal)?,
                status: enum_value(text(&row, "customer_status")?)?,
            })
        }
        "marketing" => {
            let row = variant_row(tx, "source_marketing_authority", id).await?;
            operations::SourceFact::Marketing(marketing::EngagementFact {
                campaign: marketing::CampaignRef::try_from_uuid(uuid(&row, "campaign_id")?)
                    .map_err(|_| PortError::Internal)?,
                audience_member: marketing::AudienceMemberRef::try_from_uuid(uuid(
                    &row,
                    "audience_member_id",
                )?)
                .map_err(|_| PortError::Internal)?,
                channel: code(&row, "channel_code")?,
                action: code(&row, "action_code")?,
            })
        }
        "marketing_budget_approval" => {
            let row = variant_row(tx, "source_marketing_approval_authority", id).await?;
            let amount_currency: Option<String> = row
                .try_get("amount_currency")
                .map_err(|_| PortError::Internal)?;
            let amount_minor: Option<i64> = row
                .try_get("amount_minor")
                .map_err(|_| PortError::Internal)?;
            let amount = match (amount_currency, amount_minor) {
                (Some(currency), Some(minor)) => Some(MoneyAmount::new(
                    minor,
                    CurrencyCode::try_new(&currency).map_err(|_| PortError::Internal)?,
                )),
                (None, None) => None,
                _ => return Err(PortError::Internal),
            };
            operations::SourceFact::MarketingBudgetApproval(
                marketing::CampaignBudgetApprovalRequestedFact {
                    budget: marketing::CampaignBudgetRef::try_from_uuid(uuid(&row, "budget_id")?)
                        .map_err(|_| PortError::Internal)?,
                    campaign: marketing::CampaignRef::try_from_uuid(uuid(&row, "campaign_id")?)
                        .map_err(|_| PortError::Internal)?,
                    approval: operations::ApprovalRequestFact {
                        request: operations::ApprovalRequestRef::try_from_uuid(uuid(
                            &row,
                            "approval_request_id",
                        )?)
                        .map_err(|_| PortError::Internal)?,
                        subject: RecordRef::try_new(
                            enum_value(text(&row, "subject_context")?)?,
                            text(&row, "subject_aggregate_kind")?,
                            uuid(&row, "subject_record_id")?,
                            u64::try_from(
                                row.try_get::<i64, _>("subject_aggregate_version")
                                    .map_err(|_| PortError::Internal)?,
                            )
                            .map_err(|_| PortError::Internal)?,
                        )
                        .map_err(|_| PortError::Internal)?,
                        policy_code: code(&row, "policy_code")?,
                        amount,
                        requested_by: code(&row, "requested_by")?,
                        requested_at: row
                            .try_get("requested_at")
                            .map_err(|_| PortError::Internal)?,
                    },
                },
            )
        }
        "crm" => {
            let row = variant_row(tx, "source_crm_authority", id).await?;
            operations::SourceFact::Crm(crm::OpportunityFact {
                opportunity: crm::OpportunityRef::try_from_uuid(uuid(&row, "opportunity_id")?)
                    .map_err(|_| PortError::Internal)?,
                customer: customer::CustomerRef::try_from_uuid(uuid(&row, "customer_id")?)
                    .map_err(|_| PortError::Internal)?,
                stage: enum_value(text(&row, "opportunity_stage")?)?,
                expected_value: money(&row, "expected_currency", "expected_minor")?,
            })
        }
        "procurement" => load_procurement(tx, id).await?,
        "inventory" => {
            let row = variant_row(tx, "source_inventory_authority", id).await?;
            operations::SourceFact::Inventory(inventory::StockMovementFact {
                movement: inventory::StockMovementRef::try_from_uuid(uuid(&row, "movement_id")?)
                    .map_err(|_| PortError::Internal)?,
                stock_item: inventory::StockItemRef::try_from_uuid(uuid(&row, "stock_item_id")?)
                    .map_err(|_| PortError::Internal)?,
                warehouse: inventory::WarehouseRef::try_from_uuid(uuid(&row, "warehouse_id")?)
                    .map_err(|_| PortError::Internal)?,
                item_code: code(&row, "item_code")?,
                quantity_delta: quantity(&row)?,
                procurement_line: row
                    .try_get::<Option<Uuid>, _>("procurement_line_id")
                    .map_err(|_| PortError::Internal)?
                    .map(procurement::PurchaseOrderLineRef::try_from_uuid)
                    .transpose()
                    .map_err(|_| PortError::Internal)?,
            })
        }
        "assets" => {
            let row = variant_row(tx, "source_assets_authority", id).await?;
            operations::SourceFact::Assets(assets::AssetConditionFact {
                asset: assets::AssetRef::try_from_uuid(uuid(&row, "asset_id")?)
                    .map_err(|_| PortError::Internal)?,
                asset_code: code(&row, "asset_code")?,
                name: display(&row, "asset_name")?,
                location: assets::AssetLocationRef::try_from_uuid(uuid(&row, "location_id")?)
                    .map_err(|_| PortError::Internal)?,
                condition: enum_value(text(&row, "asset_condition")?)?,
            })
        }
        "maintenance" => {
            let row = variant_row(tx, "source_maintenance_authority", id).await?;
            operations::SourceFact::Maintenance(maintenance::WorkOrderFact {
                work_order: maintenance::WorkOrderRef::try_from_uuid(uuid(&row, "work_order_id")?)
                    .map_err(|_| PortError::Internal)?,
                work_order_code: code(&row, "work_order_code")?,
                asset: assets::AssetRef::try_from_uuid(uuid(&row, "asset_id")?)
                    .map_err(|_| PortError::Internal)?,
                status: enum_value(text(&row, "work_order_status")?)?,
                scheduled_for: row
                    .try_get("scheduled_for")
                    .map_err(|_| PortError::Internal)?,
            })
        }
        "quality" => {
            let row = variant_row(tx, "source_quality_authority", id).await?;
            operations::SourceFact::Quality(quality::InspectionFact {
                inspection: quality::InspectionRef::try_from_uuid(uuid(&row, "inspection_id")?)
                    .map_err(|_| PortError::Internal)?,
                subject: quality::QualitySubjectRef::try_from_uuid(uuid(&row, "subject_id")?)
                    .map_err(|_| PortError::Internal)?,
                inspection_code: code(&row, "inspection_code")?,
                outcome: enum_value(text(&row, "inspection_outcome")?)?,
            })
        }
        "sales" => load_sales(tx, id).await?,
        "people_payroll" => {
            let row = variant_row(tx, "source_people_payroll_authority", id).await?;
            operations::SourceFact::PeoplePayroll(people_payroll::PayrollFinalizationFact {
                payroll_run: people_payroll::PayrollRunRef::try_from_uuid(uuid(
                    &row,
                    "payroll_run_id",
                )?)
                .map_err(|_| PortError::Internal)?,
                period_code: code(&row, "period_code")?,
                finalized_at: row
                    .try_get("finalized_at")
                    .map_err(|_| PortError::Internal)?,
                gross_amount: money(&row, "gross_currency", "gross_minor")?,
                deduction_amount: money(&row, "deduction_currency", "deduction_minor")?,
                net_amount: money(&row, "net_currency", "net_minor")?,
            })
        }
        "pos_sale" => {
            let row = variant_row(tx, "source_pos_sale_authority", id).await?;
            operations::SourceFact::PosSale(pos::PosSaleFact {
                transaction: pos::PosTransactionRef::try_from_uuid(uuid(&row, "transaction_id")?)
                    .map_err(|_| PortError::Internal)?,
                register: pos::RegisterRef::try_from_uuid(uuid(&row, "register_id")?)
                    .map_err(|_| PortError::Internal)?,
                receipt_code: code(&row, "receipt_code")?,
                customer: row
                    .try_get::<Option<Uuid>, _>("customer_id")
                    .map_err(|_| PortError::Internal)?
                    .map(customer::CustomerRef::try_from_uuid)
                    .transpose()
                    .map_err(|_| PortError::Internal)?,
                gross_amount: money(&row, "gross_currency", "gross_minor")?,
                completed_at: row
                    .try_get("completed_at")
                    .map_err(|_| PortError::Internal)?,
            })
        }
        "pos_refund" => {
            let row = variant_row(tx, "source_pos_refund_authority", id).await?;
            operations::SourceFact::PosRefund(pos::PosRefundFact {
                refund: pos::PosTransactionRef::try_from_uuid(uuid(&row, "refund_id")?)
                    .map_err(|_| PortError::Internal)?,
                original_sale: pos::PosTransactionRef::try_from_uuid(uuid(
                    &row,
                    "original_sale_id",
                )?)
                .map_err(|_| PortError::Internal)?,
                amount: money(&row, "amount_currency", "amount_minor")?,
                approved_at: row
                    .try_get("approved_at")
                    .map_err(|_| PortError::Internal)?,
            })
        }
        "finance" => {
            let row = variant_row(tx, "source_finance_authority", id).await?;
            operations::SourceFact::Finance(finance::JournalPostingFact {
                journal: finance::JournalRef::try_from_uuid(uuid(&row, "journal_id")?)
                    .map_err(|_| PortError::Internal)?,
                journal_code: code(&row, "journal_code")?,
                source_context: enum_value(text(&row, "journal_source_context")?)?,
                source_reference: code(&row, "source_reference")?,
                debit_total: money(&row, "debit_currency", "debit_minor")?,
                credit_total: money(&row, "credit_currency", "credit_minor")?,
                posted_at: row.try_get("posted_at").map_err(|_| PortError::Internal)?,
            })
        }
        "legal" => {
            let row = variant_row(tx, "source_legal_authority", id).await?;
            operations::SourceFact::Legal(legal::LegalObligationFact {
                obligation: legal::ObligationRef::try_from_uuid(uuid(&row, "obligation_id")?)
                    .map_err(|_| PortError::Internal)?,
                legal_case: legal::LegalCaseRef::try_from_uuid(uuid(&row, "legal_case_id")?)
                    .map_err(|_| PortError::Internal)?,
                customer: row
                    .try_get::<Option<Uuid>, _>("customer_id")
                    .map_err(|_| PortError::Internal)?
                    .map(customer::CustomerRef::try_from_uuid)
                    .transpose()
                    .map_err(|_| PortError::Internal)?,
                obligation_code: code(&row, "obligation_code")?,
                title: display(&row, "obligation_title")?,
                status: enum_value(text(&row, "obligation_status")?)?,
                due_at: row.try_get("due_at").map_err(|_| PortError::Internal)?,
            })
        }
        "support" => {
            let row = variant_row(tx, "source_support_authority", id).await?;
            operations::SourceFact::Support(support::SupportCaseFact {
                support_case: support::SupportCaseRef::try_from_uuid(uuid(
                    &row,
                    "support_case_id",
                )?)
                .map_err(|_| PortError::Internal)?,
                case_code: code(&row, "case_code")?,
                customer: customer::CustomerRef::try_from_uuid(uuid(&row, "customer_id")?)
                    .map_err(|_| PortError::Internal)?,
                status: enum_value(text(&row, "support_status")?)?,
                category: code(&row, "category_code")?,
                updated_at: row
                    .try_get("source_updated_at")
                    .map_err(|_| PortError::Internal)?,
            })
        }
        "support_incident" => load_support_incident(tx, id).await?,
        _ => return Err(PortError::Internal),
    };
    if fact.source_context() != expected_source_context(&kind)? {
        return Err(PortError::Internal);
    }
    Ok(fact)
}

pub(crate) async fn replace_source_fact(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
    aggregate_version: u64,
    fact: &operations::SourceFact,
) -> Result<(), CqrsSqlxError> {
    clear_source_fact(tx, id).await?;
    let kind = fact_kind(fact);
    sqlx::query(
        "INSERT INTO erpx_operations_write.exception_source_fact_authority
         (tenant_id,exception_id,fact_kind,source_version)
         VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3)",
    )
    .bind(id.as_uuid())
    .bind(kind)
    .bind(to_i64(aggregate_version)?)
    .execute(&mut **tx)
    .await?;
    write_source_fact(tx, id, fact).await
}

async fn clear_source_fact(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
) -> Result<(), CqrsSqlxError> {
    for table in [
        "source_procurement_line_authority",
        "source_sales_line_authority",
        "source_support_service_authority",
        "source_support_customer_authority",
    ] {
        delete_rows(tx, table, id).await?;
    }
    for table in SOURCE_ROOTS {
        delete_rows(tx, table, id).await?;
    }
    delete_rows(tx, "exception_source_fact_authority", id).await
}

async fn delete_rows(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id: ExceptionId,
) -> Result<(), CqrsSqlxError> {
    let sql = format!(
        "DELETE FROM erpx_operations_write.{table}
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND exception_id=$1"
    );
    sqlx::query(&sql)
        .bind(id.as_uuid())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn load_procurement(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
) -> Result<operations::SourceFact, PortError> {
    let row = variant_row(tx, "source_procurement_authority", id).await?;
    let lines = sqlx::query(
        "SELECT line_id,item_code,quantity_units,quantity_scale,line_currency,line_minor,
                destination_code
         FROM erpx_operations_write.source_procurement_line_authority
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND exception_id=$1
         ORDER BY line_ordinal",
    )
    .bind(id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?
    .into_iter()
    .map(|line| {
        Ok(procurement::ReleasedPurchaseOrderLineFact {
            line: procurement::PurchaseOrderLineRef::try_from_uuid(uuid(&line, "line_id")?)
                .map_err(|_| PortError::Internal)?,
            item_code: code(&line, "item_code")?,
            ordered_quantity: quantity(&line)?,
            line_value: money(&line, "line_currency", "line_minor")?,
            destination_code: code(&line, "destination_code")?,
        })
    })
    .collect::<Result<Vec<_>, PortError>>()?;
    Ok(operations::SourceFact::Procurement(
        procurement::ReleasedPurchaseOrderFact {
            purchase_order: procurement::PurchaseOrderRef::try_from_uuid(uuid(
                &row,
                "purchase_order_id",
            )?)
            .map_err(|_| PortError::Internal)?,
            supplier: procurement::SupplierRef::try_from_uuid(uuid(&row, "supplier_id")?)
                .map_err(|_| PortError::Internal)?,
            order_code: code(&row, "order_code")?,
            lines,
        },
    ))
}

async fn load_sales(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
) -> Result<operations::SourceFact, PortError> {
    let row = variant_row(tx, "source_sales_authority", id).await?;
    let lines = sqlx::query(
        "SELECT line_id,item_code,quantity_units,quantity_scale,line_currency,line_minor
         FROM erpx_operations_write.source_sales_line_authority
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND exception_id=$1
         ORDER BY line_ordinal",
    )
    .bind(id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?
    .into_iter()
    .map(|line| {
        Ok(sales::SalesOrderLineFact {
            line: sales::SalesOrderLineRef::try_from_uuid(uuid(&line, "line_id")?)
                .map_err(|_| PortError::Internal)?,
            item_code: code(&line, "item_code")?,
            quantity: quantity(&line)?,
            line_value: money(&line, "line_currency", "line_minor")?,
        })
    })
    .collect::<Result<Vec<_>, PortError>>()?;
    Ok(operations::SourceFact::Sales(sales::SalesOrderFact {
        sales_order: sales::SalesOrderRef::try_from_uuid(uuid(&row, "sales_order_id")?)
            .map_err(|_| PortError::Internal)?,
        order_code: code(&row, "order_code")?,
        customer: customer::CustomerRef::try_from_uuid(uuid(&row, "customer_id")?)
            .map_err(|_| PortError::Internal)?,
        status: enum_value(text(&row, "sales_status")?)?,
        total: money(&row, "total_currency", "total_minor")?,
        lines,
    }))
}

async fn load_support_incident(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
) -> Result<operations::SourceFact, PortError> {
    let row = variant_row(tx, "source_support_incident_authority", id).await?;
    let affected_services = sqlx::query(
        "SELECT service_code FROM erpx_operations_write.source_support_service_authority
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND exception_id=$1
         ORDER BY service_ordinal",
    )
    .bind(id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?
    .into_iter()
    .map(|service| code(&service, "service_code"))
    .collect::<Result<Vec<_>, PortError>>()?;
    let affected_customer_accounts = sqlx::query(
        "SELECT customer_account_id
         FROM erpx_operations_write.source_support_customer_authority
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND exception_id=$1
         ORDER BY customer_ordinal",
    )
    .bind(id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?
    .into_iter()
    .map(|customer| {
        customer::CustomerAccountRef::try_from_uuid(uuid(&customer, "customer_account_id")?)
            .map_err(|_| PortError::Internal)
    })
    .collect::<Result<Vec<_>, PortError>>()?;
    Ok(operations::SourceFact::SupportIncident(
        support::SupportIncidentFact {
            incident: support::SupportIncidentRef::try_from_uuid(uuid(&row, "incident_id")?)
                .map_err(|_| PortError::Internal)?,
            incident_code: code(&row, "incident_code")?,
            status: enum_value(text(&row, "incident_status")?)?,
            severity: code(&row, "severity_code")?,
            affected_services,
            affected_customer_accounts,
            updated_at: row
                .try_get("source_updated_at")
                .map_err(|_| PortError::Internal)?,
        },
    ))
}

async fn write_source_fact(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
    fact: &operations::SourceFact,
) -> Result<(), CqrsSqlxError> {
    match fact {
        operations::SourceFact::Commercial(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_commercial_authority
                 (tenant_id,exception_id,subscription_id,customer_id,plan_id,subscription_status,
                  recurring_currency,recurring_minor,effective_at)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(id.as_uuid())
            .bind(value.subscription.as_uuid())
            .bind(value.customer.as_uuid())
            .bind(value.plan.as_uuid())
            .bind(enum_name(value.status)?)
            .bind(value.recurring_amount.currency().as_str())
            .bind(value.recurring_amount.minor_units())
            .bind(value.effective_at)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Customer(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_customer_authority
                 (tenant_id,exception_id,customer_id,account_id,customer_code,display_name,
                  owning_company_id,customer_status)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(id.as_uuid())
            .bind(value.customer.as_uuid())
            .bind(value.account.as_uuid())
            .bind(value.customer_code.as_str())
            .bind(value.display_name.as_str())
            .bind(value.owning_company.as_uuid())
            .bind(enum_name(value.status)?)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Marketing(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_marketing_authority
                 (tenant_id,exception_id,campaign_id,audience_member_id,channel_code,action_code)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5)",
            )
            .bind(id.as_uuid())
            .bind(value.campaign.as_uuid())
            .bind(value.audience_member.as_uuid())
            .bind(value.channel.as_str())
            .bind(value.action.as_str())
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::MarketingBudgetApproval(value) => {
            let approval = &value.approval;
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_marketing_approval_authority
                 (tenant_id,exception_id,budget_id,campaign_id,approval_request_id,subject_record_id,
                  subject_context,subject_aggregate_kind,subject_aggregate_version,policy_code,
                  amount_currency,amount_minor,requested_by,requested_at)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
            )
            .bind(id.as_uuid())
            .bind(value.budget.as_uuid())
            .bind(value.campaign.as_uuid())
            .bind(approval.request.as_uuid())
            .bind(approval.subject.aggregate_id)
            .bind(enum_name(approval.subject.context)?)
            .bind(approval.subject.aggregate_kind.as_str())
            .bind(to_i64(approval.subject.aggregate_version)?)
            .bind(approval.policy_code.as_str())
            .bind(approval.amount.map(|amount| amount.currency().as_str().to_owned()))
            .bind(approval.amount.map(MoneyAmount::minor_units))
            .bind(approval.requested_by.as_str())
            .bind(approval.requested_at)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Crm(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_crm_authority
                 (tenant_id,exception_id,opportunity_id,customer_id,opportunity_stage,
                  expected_currency,expected_minor)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6)",
            )
            .bind(id.as_uuid())
            .bind(value.opportunity.as_uuid())
            .bind(value.customer.as_uuid())
            .bind(enum_name(value.stage)?)
            .bind(value.expected_value.currency().as_str())
            .bind(value.expected_value.minor_units())
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Procurement(value) => write_procurement(tx, id, value).await?,
        operations::SourceFact::Inventory(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_inventory_authority
                 (tenant_id,exception_id,movement_id,stock_item_id,warehouse_id,item_code,
                  quantity_units,quantity_scale,procurement_line_id)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(id.as_uuid())
            .bind(value.movement.as_uuid())
            .bind(value.stock_item.as_uuid())
            .bind(value.warehouse.as_uuid())
            .bind(value.item_code.as_str())
            .bind(value.quantity_delta.units())
            .bind(i16::from(value.quantity_delta.scale()))
            .bind(value.procurement_line.map(|line| line.as_uuid()))
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Assets(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_assets_authority
                 (tenant_id,exception_id,asset_id,asset_code,asset_name,location_id,asset_condition)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6)",
            )
            .bind(id.as_uuid())
            .bind(value.asset.as_uuid())
            .bind(value.asset_code.as_str())
            .bind(value.name.as_str())
            .bind(value.location.as_uuid())
            .bind(enum_name(value.condition)?)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Maintenance(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_maintenance_authority
                 (tenant_id,exception_id,work_order_id,work_order_code,asset_id,work_order_status,
                  scheduled_for)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6)",
            )
            .bind(id.as_uuid())
            .bind(value.work_order.as_uuid())
            .bind(value.work_order_code.as_str())
            .bind(value.asset.as_uuid())
            .bind(enum_name(value.status)?)
            .bind(value.scheduled_for)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Quality(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_quality_authority
                 (tenant_id,exception_id,inspection_id,subject_id,inspection_code,inspection_outcome)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5)",
            )
            .bind(id.as_uuid())
            .bind(value.inspection.as_uuid())
            .bind(value.subject.as_uuid())
            .bind(value.inspection_code.as_str())
            .bind(enum_name(value.outcome)?)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Sales(value) => write_sales(tx, id, value).await?,
        operations::SourceFact::PeoplePayroll(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_people_payroll_authority
                 (tenant_id,exception_id,payroll_run_id,period_code,finalized_at,gross_currency,
                  gross_minor,deduction_currency,deduction_minor,net_currency,net_minor)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(id.as_uuid())
            .bind(value.payroll_run.as_uuid())
            .bind(value.period_code.as_str())
            .bind(value.finalized_at)
            .bind(value.gross_amount.currency().as_str())
            .bind(value.gross_amount.minor_units())
            .bind(value.deduction_amount.currency().as_str())
            .bind(value.deduction_amount.minor_units())
            .bind(value.net_amount.currency().as_str())
            .bind(value.net_amount.minor_units())
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::PosSale(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_pos_sale_authority
                 (tenant_id,exception_id,transaction_id,register_id,receipt_code,customer_id,
                  gross_currency,gross_minor,completed_at)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(id.as_uuid())
            .bind(value.transaction.as_uuid())
            .bind(value.register.as_uuid())
            .bind(value.receipt_code.as_str())
            .bind(value.customer.map(|customer| customer.as_uuid()))
            .bind(value.gross_amount.currency().as_str())
            .bind(value.gross_amount.minor_units())
            .bind(value.completed_at)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::PosRefund(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_pos_refund_authority
                 (tenant_id,exception_id,refund_id,original_sale_id,amount_currency,amount_minor,
                  approved_at)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6)",
            )
            .bind(id.as_uuid())
            .bind(value.refund.as_uuid())
            .bind(value.original_sale.as_uuid())
            .bind(value.amount.currency().as_str())
            .bind(value.amount.minor_units())
            .bind(value.approved_at)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Finance(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_finance_authority
                 (tenant_id,exception_id,journal_id,journal_code,journal_source_context,
                  source_reference,debit_currency,debit_minor,credit_currency,credit_minor,posted_at)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(id.as_uuid())
            .bind(value.journal.as_uuid())
            .bind(value.journal_code.as_str())
            .bind(enum_name(value.source_context)?)
            .bind(value.source_reference.as_str())
            .bind(value.debit_total.currency().as_str())
            .bind(value.debit_total.minor_units())
            .bind(value.credit_total.currency().as_str())
            .bind(value.credit_total.minor_units())
            .bind(value.posted_at)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Legal(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_legal_authority
                 (tenant_id,exception_id,obligation_id,legal_case_id,customer_id,obligation_code,
                  obligation_title,obligation_status,due_at)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(id.as_uuid())
            .bind(value.obligation.as_uuid())
            .bind(value.legal_case.as_uuid())
            .bind(value.customer.map(|customer| customer.as_uuid()))
            .bind(value.obligation_code.as_str())
            .bind(value.title.as_str())
            .bind(enum_name(value.status)?)
            .bind(value.due_at)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::Support(value) => {
            sqlx::query(
                "INSERT INTO erpx_operations_write.source_support_authority
                 (tenant_id,exception_id,support_case_id,case_code,customer_id,support_status,
                  category_code,source_updated_at)
                 VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(id.as_uuid())
            .bind(value.support_case.as_uuid())
            .bind(value.case_code.as_str())
            .bind(value.customer.as_uuid())
            .bind(enum_name(value.status)?)
            .bind(value.category.as_str())
            .bind(value.updated_at)
            .execute(&mut **tx)
            .await?;
        }
        operations::SourceFact::SupportIncident(value) => {
            write_support_incident(tx, id, value).await?;
        }
    }
    Ok(())
}

async fn write_procurement(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
    value: &procurement::ReleasedPurchaseOrderFact,
) -> Result<(), CqrsSqlxError> {
    sqlx::query(
        "INSERT INTO erpx_operations_write.source_procurement_authority
         (tenant_id,exception_id,purchase_order_id,supplier_id,order_code)
         VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4)",
    )
    .bind(id.as_uuid())
    .bind(value.purchase_order.as_uuid())
    .bind(value.supplier.as_uuid())
    .bind(value.order_code.as_str())
    .execute(&mut **tx)
    .await?;
    for (ordinal, line) in value.lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO erpx_operations_write.source_procurement_line_authority
             (tenant_id,exception_id,line_ordinal,line_id,item_code,quantity_units,quantity_scale,
              line_currency,line_minor,destination_code)
             VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(id.as_uuid())
        .bind(i32::try_from(ordinal).map_err(|_| CqrsSqlxError::NumericRange)?)
        .bind(line.line.as_uuid())
        .bind(line.item_code.as_str())
        .bind(line.ordered_quantity.units())
        .bind(i16::from(line.ordered_quantity.scale()))
        .bind(line.line_value.currency().as_str())
        .bind(line.line_value.minor_units())
        .bind(line.destination_code.as_str())
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn write_sales(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
    value: &sales::SalesOrderFact,
) -> Result<(), CqrsSqlxError> {
    sqlx::query(
        "INSERT INTO erpx_operations_write.source_sales_authority
         (tenant_id,exception_id,sales_order_id,order_code,customer_id,sales_status,
          total_currency,total_minor)
         VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(id.as_uuid())
    .bind(value.sales_order.as_uuid())
    .bind(value.order_code.as_str())
    .bind(value.customer.as_uuid())
    .bind(enum_name(value.status)?)
    .bind(value.total.currency().as_str())
    .bind(value.total.minor_units())
    .execute(&mut **tx)
    .await?;
    for (ordinal, line) in value.lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO erpx_operations_write.source_sales_line_authority
             (tenant_id,exception_id,line_ordinal,line_id,item_code,quantity_units,quantity_scale,
              line_currency,line_minor)
             VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(id.as_uuid())
        .bind(i32::try_from(ordinal).map_err(|_| CqrsSqlxError::NumericRange)?)
        .bind(line.line.as_uuid())
        .bind(line.item_code.as_str())
        .bind(line.quantity.units())
        .bind(i16::from(line.quantity.scale()))
        .bind(line.line_value.currency().as_str())
        .bind(line.line_value.minor_units())
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn write_support_incident(
    tx: &mut Transaction<'_, Postgres>,
    id: ExceptionId,
    value: &support::SupportIncidentFact,
) -> Result<(), CqrsSqlxError> {
    sqlx::query(
        "INSERT INTO erpx_operations_write.source_support_incident_authority
         (tenant_id,exception_id,incident_id,incident_code,incident_status,severity_code,
          source_updated_at)
         VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6)",
    )
    .bind(id.as_uuid())
    .bind(value.incident.as_uuid())
    .bind(value.incident_code.as_str())
    .bind(enum_name(value.status)?)
    .bind(value.severity.as_str())
    .bind(value.updated_at)
    .execute(&mut **tx)
    .await?;
    for (ordinal, service) in value.affected_services.iter().enumerate() {
        sqlx::query(
            "INSERT INTO erpx_operations_write.source_support_service_authority
             (tenant_id,exception_id,service_ordinal,service_code)
             VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3)",
        )
        .bind(id.as_uuid())
        .bind(i32::try_from(ordinal).map_err(|_| CqrsSqlxError::NumericRange)?)
        .bind(service.as_str())
        .execute(&mut **tx)
        .await?;
    }
    for (ordinal, customer) in value.affected_customer_accounts.iter().enumerate() {
        sqlx::query(
            "INSERT INTO erpx_operations_write.source_support_customer_authority
             (tenant_id,exception_id,customer_ordinal,customer_account_id)
             VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3)",
        )
        .bind(id.as_uuid())
        .bind(i32::try_from(ordinal).map_err(|_| CqrsSqlxError::NumericRange)?)
        .bind(customer.as_uuid())
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn variant_row(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id: ExceptionId,
) -> Result<sqlx::postgres::PgRow, PortError> {
    let sql = format!(
        "SELECT * FROM erpx_operations_write.{table}
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND exception_id=$1"
    );
    sqlx::query(&sql)
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?
        .ok_or(PortError::Internal)
}

fn uuid(row: &sqlx::postgres::PgRow, column: &str) -> Result<Uuid, PortError> {
    row.try_get(column).map_err(|_| PortError::Internal)
}

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, PortError> {
    row.try_get(column).map_err(|_| PortError::Internal)
}

fn code(row: &sqlx::postgres::PgRow, column: &str) -> Result<ReferenceCode, PortError> {
    ReferenceCode::try_new(text(row, column)?).map_err(|_| PortError::Internal)
}

fn display(row: &sqlx::postgres::PgRow, column: &str) -> Result<DisplayName, PortError> {
    DisplayName::try_new(text(row, column)?).map_err(|_| PortError::Internal)
}

fn money(
    row: &sqlx::postgres::PgRow,
    currency_column: &str,
    minor_column: &str,
) -> Result<MoneyAmount, PortError> {
    Ok(MoneyAmount::new(
        row.try_get(minor_column).map_err(|_| PortError::Internal)?,
        CurrencyCode::try_new(&text(row, currency_column)?).map_err(|_| PortError::Internal)?,
    ))
}

fn quantity(row: &sqlx::postgres::PgRow) -> Result<Quantity, PortError> {
    let scale = u8::try_from(
        row.try_get::<i16, _>("quantity_scale")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    Quantity::try_new(
        row.try_get("quantity_units")
            .map_err(|_| PortError::Internal)?,
        scale,
    )
    .map_err(|_| PortError::Internal)
}

fn enum_name<T: Serialize>(value: T) -> Result<String, CqrsSqlxError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or(CqrsSqlxError::InvalidInput("source enum cannot be encoded"))
}

fn enum_value<T: DeserializeOwned>(value: String) -> Result<T, PortError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| PortError::Internal)
}

const fn fact_kind(fact: &operations::SourceFact) -> &'static str {
    match fact {
        operations::SourceFact::Commercial(_) => "commercial",
        operations::SourceFact::Customer(_) => "customer",
        operations::SourceFact::Marketing(_) => "marketing",
        operations::SourceFact::MarketingBudgetApproval(_) => "marketing_budget_approval",
        operations::SourceFact::Crm(_) => "crm",
        operations::SourceFact::Procurement(_) => "procurement",
        operations::SourceFact::Inventory(_) => "inventory",
        operations::SourceFact::Assets(_) => "assets",
        operations::SourceFact::Maintenance(_) => "maintenance",
        operations::SourceFact::Quality(_) => "quality",
        operations::SourceFact::Sales(_) => "sales",
        operations::SourceFact::PeoplePayroll(_) => "people_payroll",
        operations::SourceFact::PosSale(_) => "pos_sale",
        operations::SourceFact::PosRefund(_) => "pos_refund",
        operations::SourceFact::Finance(_) => "finance",
        operations::SourceFact::Legal(_) => "legal",
        operations::SourceFact::Support(_) => "support",
        operations::SourceFact::SupportIncident(_) => "support_incident",
    }
}

fn expected_source_context(kind: &str) -> Result<BoundedContext, PortError> {
    match kind {
        "commercial" => Ok(BoundedContext::Commercial),
        "customer" => Ok(BoundedContext::Customer),
        "marketing" | "marketing_budget_approval" => Ok(BoundedContext::Marketing),
        "crm" => Ok(BoundedContext::Crm),
        "procurement" => Ok(BoundedContext::Procurement),
        "inventory" => Ok(BoundedContext::Inventory),
        "assets" => Ok(BoundedContext::Assets),
        "maintenance" => Ok(BoundedContext::Maintenance),
        "quality" => Ok(BoundedContext::Quality),
        "sales" => Ok(BoundedContext::Sales),
        "people_payroll" => Ok(BoundedContext::PeoplePayroll),
        "pos_sale" | "pos_refund" => Ok(BoundedContext::Pos),
        "finance" => Ok(BoundedContext::Finance),
        "legal" => Ok(BoundedContext::Legal),
        "support" | "support_incident" => Ok(BoundedContext::Support),
        _ => Err(PortError::Internal),
    }
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
