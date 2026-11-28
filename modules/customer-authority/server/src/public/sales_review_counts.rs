//! Native owner port for the sales customer review counts read.
//!
//! The PostgreSQL adapter executes the SQL; this HTTP router is the mount
//! point composition binds for the sales review-count route family. No
//! TypeScript statement mirror and no fallback remain for this operation.
pub use crate::adapters::sales_review_counts_http::{
    CUSTOMER_SALES_REVIEW_COUNTS_PATH, SalesReviewCountsBackend, SalesReviewCountsParams,
    SalesReviewCountsRequest, SalesReviewCountsTarget, sales_review_counts_router,
};
pub use crate::adapters::sales_review_counts_postgres::PgSalesReviewCounts;
pub use crate::application::sales_review_counts::{
    ReviewCountsFuture, SalesReviewCountsSource, sales_customer_review_counts,
};
pub use crate::domain::sales_review_counts::{
    REVIEW_KINDS, ReviewCount, SalesReviewCountsQuery, invalid, unavailable, validate_counts,
};
