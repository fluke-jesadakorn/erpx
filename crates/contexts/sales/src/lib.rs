#![doc = "Sales bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AcceptQuote, AcceptRevenueFulfillment, ActivateRevenueArrangement, CancelRevenueArrangement,
    CancelSalesOrder, ConfirmSalesOrder, CreateQuote, CreateSalesOrder, ExpireQuote, GetQuote,
    GetRevenueArrangement, GetSalesOrder, ListSalesOrders, MarkSalesOrderFulfilled,
    MarkSalesOrderPartiallyFulfilled, RejectQuote, SalesApplicationError, SalesCommand,
    SalesCommandHandler, SalesMutationResult, SalesQuery, SalesQueryHandler, SalesQueryResponse,
    SendQuote,
};
pub use domain::{
    AcceptedFulfillment, BillingEventId, FulfillmentId, PerformanceObligation,
    PerformanceObligationDefinition, PerformanceObligationId, Quote, QuoteId, QuoteStatus,
    RevenueArrangement, RevenueArrangementId, RevenueArrangementStatus, SalesAggregate,
    SalesDomainError, SalesOrder, SalesOrderId, SalesOrderLine, SalesOrderLineId, SalesOrderStatus,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, EntitlementPort, PortError, QuoteView, ReadRepository,
    RevenueArrangementView, SalesAction, SalesCommit, SalesOrderList, SalesOrderListFilter,
    SalesOrderView, StoredCommit, WriteRepository,
};
