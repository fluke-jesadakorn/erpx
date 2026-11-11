//! Messaging owner HTTP ingress including teams, channels and the Files port.
pub const CONTRACT_VERSION: &str = "collaboration.messaging.http.v1";

pub use crate::adapters::messaging_http::{
    FileQuery, HistoryQuery, MessagingAttachmentKind, MessagingAttachmentReply,
    MessagingChannelsData, MessagingFileBinding, MessagingFileContent, MessagingFilesBindRequest,
    MessagingFilesContentRequest, MessagingFilesPort, MessagingHistoryData,
    MessagingHttpAttachmentRequest, MessagingHttpBackend, MessagingHttpCommand,
    MessagingHttpCommandRequest, MessagingHttpFuture, MessagingHttpReadReply,
    MessagingHttpReadRequest, MessagingHttpReceipt, MessagingHttpState, MessagingHttpTarget,
    MessagingMembersData, MessagingOverviewData, MessagingReadKind, bind_attachment,
    create_channel, create_room, create_team, mark_read, messaging_router, read_channels,
    read_file, read_history, read_overview, read_room_members, read_team_members, send_message,
};
pub use crate::adapters::messaging_http_wire::{
    DecodedMessagingCommand, decode_room_command, decode_team_command,
};
pub use crate::adapters::messaging_teams::{ChannelView, CurrentTeamEffects};
