use async_trait::async_trait;
use chrono::NaiveDate;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_finance::{
    Account, AccountCategory, AccountId, AccountStatus, FinanceCommand, FinanceMutationResult,
    FinanceQuery, FinanceQueryResponse, JournalId, JournalLine, JournalLineId, JournalSource,
    JournalStatus, NormalBalance, PeriodId, PeriodStatus, StatutoryTaxFact, StatutoryTaxKind,
    SubledgerId, SubledgerKind, SubledgerStatus,
    ports::{
        BankTransactionList, BankTransactionView, ChartView, JournalList, JournalView, PeriodView,
        StatutoryTaxFactView, StatutoryTaxReport, SubledgerList, SubledgerView, TrialBalanceLine,
        TrialBalanceView,
    },
};
use erpx_integration_contracts::{customer, procurement};
use erpx_proto::platform as wire;
use erpx_ui_foundation::UiError;

use crate::{features::finance::FinanceProtoTransport, transport::typed_cqrs::GrpcChannel};

use super::common::{
    AuthenticatedRoute, Wave8Route, command_result, decode_scope, invalid_response, query_result,
    status_ui, timestamp, unsupported_ui, uuid,
};

type CommandClient = wire::finance_command_service_client::FinanceCommandServiceClient<GrpcChannel>;
type QueryClient = wire::finance_query_service_client::FinanceQueryServiceClient<GrpcChannel>;

#[derive(Clone)]
pub(crate) struct FinanceGrpcTransport {
    auth: AuthenticatedRoute,
    commands: CommandClient,
    queries: QueryClient,
}

impl FinanceGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl Into<String>,
        route: Wave8Route,
    ) -> Result<Self, UiError> {
        Ok(Self {
            auth: AuthenticatedRoute::try_new(access_token, route)?,
            commands: CommandClient::new(channel.clone()),
            queries: QueryClient::new(channel),
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl FinanceProtoTransport for FinanceGrpcTransport {
    async fn command(
        &self,
        envelope: CommandEnvelope<FinanceCommand>,
    ) -> Result<CommandReceipt<FinanceMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command_id = envelope.command_id;
        let correlation_id = envelope.correlation_id;
        let mut client = self.commands.clone();
        match envelope.command {
            FinanceCommand::CreateChart(value) => {
                let request = wire::CreateFinanceChartRequest {
                    metadata: Some(metadata),
                    chart_id: value.chart_id.as_uuid().to_string(),
                    target_scope: Some(super::common::encode_scope(&value.scope)),
                    code: value.code,
                    name: value.name,
                    currency: value.currency,
                    accounts: value.accounts.iter().map(encode_account).collect(),
                };
                let response = client
                    .create_chart(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance create chart", status))?
                    .into_inner();
                let result = FinanceMutationResult::Chart(decode_chart(
                    response
                        .chart
                        .ok_or_else(|| invalid_response("decode finance chart"))?,
                )?);
                command_result(response.receipt, result, command_id, correlation_id)
            }
            FinanceCommand::AddAccount(value) => {
                let request = wire::AddFinanceAccountRequest {
                    metadata: Some(metadata),
                    chart_id: value.chart_id.as_uuid().to_string(),
                    account: Some(encode_account(&value.account)),
                };
                let response = client
                    .add_account(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance add account", status))?
                    .into_inner();
                let result = FinanceMutationResult::Chart(decode_chart(
                    response
                        .chart
                        .ok_or_else(|| invalid_response("decode finance chart"))?,
                )?);
                command_result(response.receipt, result, command_id, correlation_id)
            }
            FinanceCommand::ChangeAccountStatus(value) => {
                let request = wire::ChangeFinanceAccountStatusRequest {
                    metadata: Some(metadata),
                    chart_id: value.chart_id.as_uuid().to_string(),
                    account_id: value.account_id.as_uuid().to_string(),
                    status: encode_account_status(value.status),
                };
                let response = client
                    .change_account_status(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance change account status", status))?
                    .into_inner();
                let result = FinanceMutationResult::Chart(decode_chart(
                    response
                        .chart
                        .ok_or_else(|| invalid_response("decode finance chart"))?,
                )?);
                command_result(response.receipt, result, command_id, correlation_id)
            }
            FinanceCommand::CreatePeriod(value) => {
                let request = wire::CreateFinancePeriodRequest {
                    metadata: Some(metadata),
                    period_id: value.period_id.as_uuid().to_string(),
                    target_scope: Some(super::common::encode_scope(&value.scope)),
                    code: value.code,
                    starts_on: value.starts_on.to_string(),
                    ends_on: value.ends_on.to_string(),
                };
                let response = client
                    .create_period(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance create period", status))?
                    .into_inner();
                period_command(response, command_id, correlation_id)
            }
            FinanceCommand::BeginPeriodClose(value) => {
                let request = wire::FinancePeriodIdentityRequest {
                    metadata: Some(metadata),
                    period_id: value.period_id.as_uuid().to_string(),
                };
                let response = client
                    .begin_period_close(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance begin period close", status))?
                    .into_inner();
                period_command(response, command_id, correlation_id)
            }
            FinanceCommand::ClosePeriod(value) => {
                let request = wire::FinancePeriodIdentityRequest {
                    metadata: Some(metadata),
                    period_id: value.period_id.as_uuid().to_string(),
                };
                let response = client
                    .close_period(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance close period", status))?
                    .into_inner();
                period_command(response, command_id, correlation_id)
            }
            FinanceCommand::ReopenPeriod(value) => {
                let request = wire::FinancePeriodIdentityRequest {
                    metadata: Some(metadata),
                    period_id: value.period_id.as_uuid().to_string(),
                };
                let response = client
                    .reopen_period(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance reopen period", status))?
                    .into_inner();
                period_command(response, command_id, correlation_id)
            }
            FinanceCommand::CreateJournal(value) => {
                let JournalSource::Manual { reference } = value.source else {
                    // Integration-owned journal facts enter Finance through
                    // process managers, not through an interactive Console.
                    return Err(unsupported_ui("finance non-manual journal command"));
                };
                let request = wire::CreateTypedFinanceJournalRequest {
                    metadata: Some(metadata),
                    journal_id: value.journal_id.as_uuid().to_string(),
                    target_scope: Some(super::common::encode_scope(&value.scope)),
                    chart_id: value.chart_id.as_uuid().to_string(),
                    period_id: value.period_id.as_uuid().to_string(),
                    journal_code: value.journal_code,
                    entry_date: value.entry_date.to_string(),
                    currency: value.currency,
                    source: Some(wire::FinanceJournalSource {
                        source: Some(wire::finance_journal_source::Source::Manual(
                            wire::FinanceManualJournalSource { reference },
                        )),
                    }),
                    description: value.description,
                    lines: value.lines.iter().map(encode_line).collect(),
                };
                let response = client
                    .create_journal(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance create journal", status))?
                    .into_inner();
                journal_command(response, command_id, correlation_id)
            }
            FinanceCommand::PostJournal(value) => {
                let request = wire::PostTypedFinanceJournalRequest {
                    metadata: Some(metadata),
                    journal_id: value.journal_id.as_uuid().to_string(),
                    expected_chart_version: value.expected_chart_version,
                    expected_period_version: value.expected_period_version,
                };
                let response = client
                    .post_journal(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance post journal", status))?
                    .into_inner();
                journal_command(response, command_id, correlation_id)
            }
            FinanceCommand::CreateCustomerInvoiceFromBilling(_)
            | FinanceCommand::PostCustomerInvoice(_)
            | FinanceCommand::CancelCustomerInvoice(_)
            | FinanceCommand::CreateRevenueScheduleFromArrangement(_)
            | FinanceCommand::RecognizeRevenueFromFulfillment(_)
            | FinanceCommand::CreateReceivableFromInvoice(_)
            | FinanceCommand::CreatePayableFromProcurement(_) => {
                // These commands are process-manager inbound operations and
                // intentionally have no public Console RPC.
                Err(unsupported_ui(
                    "finance invoice or integration subledger command",
                ))
            }
            FinanceCommand::ApplySubledgerSettlement(value) => {
                let request = wire::ApplyFinanceSubledgerSettlementRequest {
                    metadata: Some(metadata),
                    subledger_id: value.subledger_id.as_uuid().to_string(),
                    amount_minor: value.amount_minor,
                };
                let response = client
                    .apply_subledger_settlement(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance settle subledger", status))?
                    .into_inner();
                let result = FinanceMutationResult::Subledger(decode_subledger(
                    response
                        .subledger
                        .ok_or_else(|| invalid_response("decode finance subledger"))?,
                )?);
                command_result(response.receipt, result, command_id, correlation_id)
            }
        }
    }

    async fn query(
        &self,
        envelope: QueryEnvelope<FinanceQuery>,
    ) -> Result<QueryResult<FinanceQueryResponse>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let mut client = self.queries.clone();
        match envelope.query {
            FinanceQuery::GetChart(value) => {
                let request = wire::GetFinanceChartRecordRequest {
                    metadata: Some(metadata),
                    chart_id: value.chart_id.as_uuid().to_string(),
                };
                let response = client
                    .get_chart(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance get chart", status))?
                    .into_inner();
                let value = response.chart.map(decode_chart).transpose()?;
                query_result(
                    response.metadata,
                    FinanceQueryResponse::Chart(value),
                    correlation,
                    consistency,
                )
            }
            FinanceQuery::GetPeriod(value) => {
                let request = wire::GetFinancePeriodRecordRequest {
                    metadata: Some(metadata),
                    period_id: value.period_id.as_uuid().to_string(),
                };
                let response = client
                    .get_period(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance get period", status))?
                    .into_inner();
                let value = response.period.map(decode_period).transpose()?;
                query_result(
                    response.metadata,
                    FinanceQueryResponse::Period(value),
                    correlation,
                    consistency,
                )
            }
            FinanceQuery::GetJournal(value) => {
                let request = wire::GetFinanceJournalRecordRequest {
                    metadata: Some(metadata),
                    journal_id: value.journal_id.as_uuid().to_string(),
                };
                let response = client
                    .get_journal(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance get journal", status))?
                    .into_inner();
                let value = response.journal.map(decode_journal).transpose()?;
                query_result(
                    response.metadata,
                    FinanceQueryResponse::Journal(value),
                    correlation,
                    consistency,
                )
            }
            FinanceQuery::ListJournals(value) => {
                let request = wire::ListTypedFinanceJournalsRequest {
                    metadata: Some(metadata),
                    status: value.filter.status.map(encode_journal_status),
                    period_id: value
                        .filter
                        .period_id
                        .map(|value| value.as_uuid().to_string()),
                    page_size: u32::from(value.filter.page_size),
                    cursor: value.filter.cursor,
                };
                let response = client
                    .list_journals(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance list journals", status))?
                    .into_inner();
                let value = JournalList {
                    journals: response
                        .journals
                        .into_iter()
                        .map(decode_journal)
                        .collect::<Result<_, _>>()?,
                    next_cursor: response.next_cursor,
                };
                query_result(
                    response.metadata,
                    FinanceQueryResponse::Journals(value),
                    correlation,
                    consistency,
                )
            }
            FinanceQuery::GetSubledger(value) => {
                let request = wire::GetFinanceSubledgerRecordRequest {
                    metadata: Some(metadata),
                    subledger_id: value.subledger_id.as_uuid().to_string(),
                };
                let response = client
                    .get_subledger(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance get subledger", status))?
                    .into_inner();
                let value = response.subledger.map(decode_subledger).transpose()?;
                query_result(
                    response.metadata,
                    FinanceQueryResponse::Subledger(value),
                    correlation,
                    consistency,
                )
            }
            FinanceQuery::GetCustomerInvoice(_) => {
                Err(unsupported_ui("finance customer invoice query"))
            }
            FinanceQuery::GetRevenueSchedule(_) => {
                Err(unsupported_ui("finance revenue schedule query"))
            }
            FinanceQuery::ListSubledger(value) => {
                let request = wire::ListTypedFinanceSubledgerRequest {
                    metadata: Some(metadata),
                    status: value.filter.status.map(encode_subledger_status),
                    receivable_only: value.filter.receivable_only,
                    page_size: u32::from(value.filter.page_size),
                    cursor: value.filter.cursor,
                };
                let response = client
                    .list_subledger(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance list subledger", status))?
                    .into_inner();
                let value = SubledgerList {
                    items: response
                        .items
                        .into_iter()
                        .map(decode_subledger)
                        .collect::<Result<_, _>>()?,
                    next_cursor: response.next_cursor,
                };
                query_result(
                    response.metadata,
                    FinanceQueryResponse::SubledgerList(value),
                    correlation,
                    consistency,
                )
            }
            FinanceQuery::GetTrialBalance(value) => {
                let request = wire::GetTypedFinanceTrialBalanceRequest {
                    metadata: Some(metadata),
                    chart_id: value.chart_id.as_uuid().to_string(),
                };
                let response = client
                    .get_trial_balance(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance get trial balance", status))?
                    .into_inner();
                let value = decode_trial_balance(
                    response
                        .trial_balance
                        .ok_or_else(|| invalid_response("decode finance trial balance"))?,
                )?;
                query_result(
                    response.metadata,
                    FinanceQueryResponse::TrialBalance(value),
                    correlation,
                    consistency,
                )
            }
            FinanceQuery::ListBankTransactions(value) => {
                let request = wire::ListTypedFinanceBankTransactionsRequest {
                    metadata: Some(metadata),
                    posted_from: value.filter.posted_from.map(|value| value.to_string()),
                    posted_through: value.filter.posted_through.map(|value| value.to_string()),
                    page_size: u32::from(value.filter.page_size),
                    cursor: value.filter.cursor,
                };
                let response = client
                    .list_bank_transactions(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance list bank transactions", status))?
                    .into_inner();
                let value = BankTransactionList {
                    transactions: response
                        .transactions
                        .into_iter()
                        .map(decode_bank_transaction)
                        .collect::<Result<_, _>>()?,
                    next_cursor: response.next_cursor,
                };
                query_result(
                    response.metadata,
                    FinanceQueryResponse::BankTransactions(value),
                    correlation,
                    consistency,
                )
            }
            FinanceQuery::GetStatutoryTaxReport(value) => {
                let request = wire::GetTypedFinanceStatutoryTaxReportRequest {
                    metadata: Some(metadata),
                    tax_period: value.tax_period,
                    currency: value.currency,
                };
                let response = client
                    .get_statutory_tax_report(self.auth.request(request, &actor)?)
                    .await
                    .map_err(|status| status_ui("finance get tax report", status))?
                    .into_inner();
                let value = decode_tax_report(
                    response
                        .report
                        .ok_or_else(|| invalid_response("decode finance tax report"))?,
                )?;
                query_result(
                    response.metadata,
                    FinanceQueryResponse::StatutoryTaxReport(value),
                    correlation,
                    consistency,
                )
            }
        }
    }
}

fn period_command(
    response: wire::FinancePeriodMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation_id: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<FinanceMutationResult>, UiError> {
    let result = FinanceMutationResult::Period(decode_period(
        response
            .period
            .ok_or_else(|| invalid_response("decode finance period"))?,
    )?);
    command_result(response.receipt, result, command_id, correlation_id)
}

fn journal_command(
    response: wire::FinanceJournalMutationResponse,
    command_id: erpx_kernel::CommandId,
    correlation_id: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<FinanceMutationResult>, UiError> {
    let result = FinanceMutationResult::Journal(decode_journal(
        response
            .journal
            .ok_or_else(|| invalid_response("decode finance journal"))?,
    )?);
    command_result(response.receipt, result, command_id, correlation_id)
}

fn encode_account(value: &Account) -> wire::FinanceAccountRecord {
    wire::FinanceAccountRecord {
        account_id: value.account_id.as_uuid().to_string(),
        code: value.code.clone(),
        name: value.name.clone(),
        category: encode_account_category(value.category),
        normal_balance: encode_normal_balance(value.normal_balance),
        parent_account_id: value
            .parent_account_id
            .map(|value| value.as_uuid().to_string()),
        status: encode_account_status(value.status),
    }
}

fn encode_line(value: &JournalLine) -> wire::FinanceJournalLineRecord {
    wire::FinanceJournalLineRecord {
        line_id: value.line_id.as_uuid().to_string(),
        account_id: value.account_id.as_uuid().to_string(),
        debit_minor: value.debit_minor,
        credit_minor: value.credit_minor,
        currency: value.currency.clone(),
        memo: value.memo.clone(),
    }
}

fn decode_account(value: wire::FinanceAccountRecord) -> Result<Account, UiError> {
    let account = Account {
        account_id: AccountId::try_from_uuid(uuid(&value.account_id, "decode finance account")?)
            .map_err(|_| invalid_response("decode finance account"))?,
        code: value.code,
        name: value.name,
        category: decode_account_category(value.category)?,
        normal_balance: decode_normal_balance(value.normal_balance)?,
        parent_account_id: value
            .parent_account_id
            .map(|value| {
                AccountId::try_from_uuid(uuid(&value, "decode finance account")?)
                    .map_err(|_| invalid_response("decode finance account"))
            })
            .transpose()?,
        status: decode_account_status(value.status)?,
    };
    account
        .validate()
        .map_err(|_| invalid_response("decode finance account"))?;
    Ok(account)
}

fn decode_chart(value: wire::FinanceChartRecord) -> Result<ChartView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode finance chart"));
    }
    Ok(ChartView {
        chart_id: erpx_finance::ChartId::try_from_uuid(uuid(
            &value.chart_id,
            "decode finance chart",
        )?)
        .map_err(|_| invalid_response("decode finance chart"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        code: value.code,
        name: value.name,
        currency: value.currency,
        accounts: value
            .accounts
            .into_iter()
            .map(decode_account)
            .collect::<Result<_, _>>()?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_period(value: wire::FinancePeriodRecord) -> Result<PeriodView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode finance period"));
    }
    Ok(PeriodView {
        period_id: PeriodId::try_from_uuid(uuid(&value.period_id, "decode finance period")?)
            .map_err(|_| invalid_response("decode finance period"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        code: value.code,
        starts_on: date(&value.starts_on)?,
        ends_on: date(&value.ends_on)?,
        status: decode_period_status(value.status)?,
        close_requested_by: value.close_requested_by,
        closed_by: value.closed_by,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_line(value: wire::FinanceJournalLineRecord) -> Result<JournalLine, UiError> {
    let line = JournalLine {
        line_id: JournalLineId::try_from_uuid(uuid(&value.line_id, "decode finance journal")?)
            .map_err(|_| invalid_response("decode finance journal"))?,
        account_id: AccountId::try_from_uuid(uuid(&value.account_id, "decode finance journal")?)
            .map_err(|_| invalid_response("decode finance journal"))?,
        debit_minor: value.debit_minor,
        credit_minor: value.credit_minor,
        currency: value.currency,
        memo: value.memo,
    };
    line.validate()
        .map_err(|_| invalid_response("decode finance journal"))?;
    Ok(line)
}

fn decode_journal(value: wire::FinanceJournalRecord) -> Result<JournalView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode finance journal"));
    }
    let source = value
        .manual_source
        .map(|value| JournalSource::Manual {
            reference: value.reference,
        })
        // The current typed record cannot faithfully represent integration
        // provenance. Fail closed instead of fabricating a manual source.
        .ok_or_else(|| invalid_response("decode finance journal source"))?;
    Ok(JournalView {
        journal_id: JournalId::try_from_uuid(uuid(&value.journal_id, "decode finance journal")?)
            .map_err(|_| invalid_response("decode finance journal"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        chart_id: erpx_finance::ChartId::try_from_uuid(uuid(
            &value.chart_id,
            "decode finance journal",
        )?)
        .map_err(|_| invalid_response("decode finance journal"))?,
        period_id: PeriodId::try_from_uuid(uuid(&value.period_id, "decode finance journal")?)
            .map_err(|_| invalid_response("decode finance journal"))?,
        journal_code: value.journal_code,
        entry_date: date(&value.entry_date)?,
        currency: value.currency,
        source,
        description: value.description,
        lines: value
            .lines
            .into_iter()
            .map(decode_line)
            .collect::<Result<_, _>>()?,
        debit_total_minor: value.debit_total_minor,
        credit_total_minor: value.credit_total_minor,
        status: decode_journal_status(value.status)?,
        posted_at: value.posted_at.map(|value| timestamp(&value)).transpose()?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_subledger(value: wire::FinanceSubledgerRecord) -> Result<SubledgerView, UiError> {
    if value.source_version == 0 {
        return Err(invalid_response("decode finance subledger"));
    }
    let party = uuid(&value.party_id, "decode finance subledger")?;
    let kind = match wire::FinanceSubledgerKind::try_from(value.kind).ok() {
        Some(wire::FinanceSubledgerKind::Receivable) => SubledgerKind::Receivable(
            customer::CustomerRef::try_from_uuid(party)
                .map_err(|_| invalid_response("decode finance subledger"))?,
        ),
        Some(wire::FinanceSubledgerKind::Payable) => SubledgerKind::Payable(
            procurement::SupplierRef::try_from_uuid(party)
                .map_err(|_| invalid_response("decode finance subledger"))?,
        ),
        Some(wire::FinanceSubledgerKind::Unspecified) | None => {
            return Err(invalid_response("decode finance subledger"));
        }
    };
    Ok(SubledgerView {
        subledger_id: SubledgerId::try_from_uuid(uuid(
            &value.subledger_id,
            "decode finance subledger",
        )?)
        .map_err(|_| invalid_response("decode finance subledger"))?,
        version: value.source_version,
        scope: decode_scope(value.scope)?,
        kind,
        document_code: value.document_code,
        source_reference: value.source_reference,
        currency: value.currency,
        original_minor: value.original_minor,
        outstanding_minor: value.outstanding_minor,
        status: decode_subledger_status(value.status)?,
        created_at: timestamp(&value.created_at)?,
        updated_at: timestamp(&value.updated_at)?,
    })
}

fn decode_trial_balance(
    value: wire::FinanceTrialBalanceRecord,
) -> Result<TrialBalanceView, UiError> {
    Ok(TrialBalanceView {
        scope: decode_scope(value.scope)?,
        chart_id: erpx_finance::ChartId::try_from_uuid(uuid(
            &value.chart_id,
            "decode finance trial balance",
        )?)
        .map_err(|_| invalid_response("decode finance trial balance"))?,
        currency: value.currency,
        lines: value
            .lines
            .into_iter()
            .map(|line| {
                Ok(TrialBalanceLine {
                    account_id: AccountId::try_from_uuid(uuid(
                        &line.account_id,
                        "decode finance trial balance",
                    )?)
                    .map_err(|_| invalid_response("decode finance trial balance"))?,
                    account_code: line.account_code,
                    account_name: line.account_name,
                    debit_minor: line.debit_minor,
                    credit_minor: line.credit_minor,
                    net_minor: line.net_minor,
                })
            })
            .collect::<Result<_, UiError>>()?,
        total_debit_minor: value.total_debit_minor,
        total_credit_minor: value.total_credit_minor,
    })
}

fn decode_bank_transaction(
    value: wire::FinanceBankTransactionRecord,
) -> Result<BankTransactionView, UiError> {
    let fact = value
        .fact
        .ok_or_else(|| invalid_response("decode finance bank transaction"))?;
    Ok(BankTransactionView {
        scope: decode_scope(value.scope)?,
        journal_id: JournalId::try_from_uuid(uuid(
            &value.journal_id,
            "decode finance bank transaction",
        )?)
        .map_err(|_| invalid_response("decode finance bank transaction"))?,
        fact: erpx_finance::BankTransactionFact {
            import_id: uuid(&fact.import_id, "decode finance bank transaction")?,
            transaction_id: uuid(&fact.transaction_id, "decode finance bank transaction")?,
            reference: fact.reference,
            posted_on: date(&fact.posted_on)?,
            amount_minor: fact.amount_minor,
            currency: fact.currency,
        },
    })
}

fn decode_tax_report(
    value: wire::FinanceStatutoryTaxReportRecord,
) -> Result<StatutoryTaxReport, UiError> {
    Ok(StatutoryTaxReport {
        scope: decode_scope(value.scope)?,
        tax_period: value.tax_period,
        currency: value.currency,
        facts: value
            .facts
            .into_iter()
            .map(|value| {
                let fact = value
                    .fact
                    .ok_or_else(|| invalid_response("decode finance tax fact"))?;
                Ok(StatutoryTaxFactView {
                    scope: decode_scope(value.scope)?,
                    journal_id: JournalId::try_from_uuid(uuid(
                        &value.journal_id,
                        "decode finance tax fact",
                    )?)
                    .map_err(|_| invalid_response("decode finance tax fact"))?,
                    fact: StatutoryTaxFact {
                        tax_fact_id: uuid(&fact.tax_fact_id, "decode finance tax fact")?,
                        reference: fact.reference,
                        tax_date: date(&fact.tax_date)?,
                        tax_period: fact.tax_period,
                        kind: decode_tax_kind(fact.kind)?,
                        currency: fact.currency,
                        base_minor: fact.base_minor,
                        tax_minor: fact.tax_minor,
                        tax_rate_basis_points: fact.tax_rate_basis_points,
                        tax_identifier: fact.tax_identifier,
                    },
                })
            })
            .collect::<Result<_, UiError>>()?,
        output_vat_minor: value.output_vat_minor,
        input_vat_minor: value.input_vat_minor,
        withholding_minor: value.withholding_minor,
    })
}

fn date(value: &str) -> Result<NaiveDate, UiError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| invalid_response("decode finance date"))
}

macro_rules! enum_codec {
    ($decode:ident, $encode:ident, $wire:ty, $domain:ty, {$($variant:ident),+ $(,)?}) => {
        fn $decode(value: i32) -> Result<$domain, UiError> {
            match <$wire>::try_from(value).ok() {
                $(Some(<$wire>::$variant) => Ok(<$domain>::$variant),)+
                _ => Err(invalid_response("decode finance enum")),
            }
        }
        fn $encode(value: $domain) -> i32 {
            match value {
                $(<$domain>::$variant => <$wire>::$variant as i32,)+
            }
        }
    };
}

enum_codec!(decode_account_category, encode_account_category, wire::FinanceAccountCategory, AccountCategory, {Asset, Liability, Equity, Revenue, Expense});
enum_codec!(decode_normal_balance, encode_normal_balance, wire::FinanceNormalBalance, NormalBalance, {Debit, Credit});
enum_codec!(decode_account_status, encode_account_status, wire::FinanceAccountStatus, AccountStatus, {Active, Inactive});
enum_codec!(decode_period_status, encode_period_status, wire::FinancePeriodStatus, PeriodStatus, {Open, Closing, Closed});
enum_codec!(decode_journal_status, encode_journal_status, wire::FinanceJournalStatus, JournalStatus, {Draft, Posted});
enum_codec!(decode_subledger_status, encode_subledger_status, wire::FinanceSubledgerStatus, SubledgerStatus, {Open, PartiallySettled, Settled});
enum_codec!(decode_tax_kind, encode_tax_kind, wire::FinanceStatutoryTaxKind, StatutoryTaxKind, {OutputVat, InputVat, Withholding});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_finance_statuses_round_trip() {
        for value in [
            SubledgerStatus::Open,
            SubledgerStatus::PartiallySettled,
            SubledgerStatus::Settled,
        ] {
            assert_eq!(
                decode_subledger_status(encode_subledger_status(value)),
                Ok(value)
            );
        }
    }

    #[test]
    fn journal_without_provenance_fails_closed() {
        let record = wire::FinanceJournalRecord {
            source_version: 1,
            ..Default::default()
        };
        assert!(decode_journal(record).is_err());
    }
}
