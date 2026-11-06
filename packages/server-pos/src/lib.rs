//! POS owner. Native command slices record cash movements, register-session
//! open/close/review, bills, unpaid voids and refund requests/rejections.
//! Payment/complete/refund-approval effects, Commerce/Inventory stock and
//! Finance journal/payment binding remain explicit composition ports and are
//! not invented here. Stripe and package enforcement remain in the retained
//! TypeScript owner until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
