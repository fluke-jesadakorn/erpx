#![doc = "Target-neutral UI application contracts for ERPX consoles."]

mod channel;
mod error;
mod fake;
mod flow;
mod gateway;
mod presenter;
mod session;

pub use channel::{
    ChannelDescriptor, ChannelKind, ChannelPolicy, ChannelPolicyError, RuntimeTarget,
};
pub use error::{MAX_UI_ERROR_CODE_LENGTH, MAX_UI_ERROR_MESSAGE_LENGTH, UiError, UiErrorKind};
pub use fake::{FakeCommandGateway, FakeQueryGateway};
pub use flow::{CommandThenQuery, ReadYourWritesCoordinator, ReadYourWritesRequest};
pub use gateway::{CommandGateway, QueryGateway};
pub use presenter::{Presenter, PresenterEvent, PresenterState};
pub use session::{RequestContextBuilder, RequestContextError, SessionContext};
