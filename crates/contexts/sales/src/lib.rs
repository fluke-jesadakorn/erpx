#![doc = "Sales bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    AcceptQuote, CancelSalesOrder, ConfirmSalesOrder, CreateQuote, CreateSalesOrder, ExpireQuote,
    GetQuote, GetSalesOrder, ListSalesOrders, MarkSalesOrderFulfilled,
    MarkSalesOrderPartiallyFulfilled, RejectQuote, SalesApplicationError, SalesCommand,
    SalesCommandHandler, SalesMutationResult, SalesQuery, SalesQueryHandler, SalesQueryResponse,
    SendQuote,
};
pub use domain::{
    Quote, QuoteId, QuoteStatus, SalesAggregate, SalesDomainError, SalesOrder, SalesOrderId,
    SalesOrderLine, SalesOrderLineId, SalesOrderStatus,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, EntitlementPort, PortError, QuoteView, ReadRepository,
    SalesAction, SalesCommit, SalesOrderList, SalesOrderListFilter, SalesOrderView, StoredCommit,
    WriteRepository,
};
