//! POS owner. Native command slices record cash movements, register-session
//! open/close/review, bills, unpaid voids and refund requests/rejections. The
//! native workspace read serves the current setting, registers, shift sessions,
//! cash movements and bounded recent sales/payment facts behind the caller's
//! serializable owner unit and composition-supplied package decision.
//! Payment/complete/refund-approval effects, Commerce/Inventory stock and
//! Finance journal/payment binding remain explicit composition ports and are
//! not invented here. Stripe, purchase reads and the remaining POS reads remain
//! in the retained TypeScript owner until their native slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
