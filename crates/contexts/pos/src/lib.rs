#![doc = "Point-of-sale bounded-context core."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    ApproveRefund, CancelRefund, CompleteSale, CreateSale, GetRefund, GetSale, HoldSale,
    ListRefunds, ListSales, OpenSale, PosApplicationError, PosQuery, PosQueryHandler,
    PosQueryResponse, PosRefundCommand, PosRefundCommandHandler, PosRefundMutationResult,
    PosSaleCommand, PosSaleCommandHandler, PosSaleMutationResult, PostRefund, RecordPayment,
    RejectRefund, RequestRefund, ResumeSale, VoidSale,
};
pub use domain::{
    CustomerId, Payment, PaymentId, PaymentMethod, PosDomainError, PosRefund, RefundId, RefundLine,
    RefundStatus, RegisterId, Sale, SaleId, SaleLine, SaleLineId, SaleStatus,
};
pub use ports::{
    AuthorizationPort, CommandFingerprint, CommandIdentity, EntitlementPort, PortError, PosAction,
    PosReadRepository, RefundCommit, RefundList, RefundListFilter, RefundStoredCommit,
    RefundUnitOfWork, RefundView, SaleCommit, SaleList, SaleListFilter, SaleStoredCommit,
    SaleUnitOfWork, SaleView, WriteRepository,
};
