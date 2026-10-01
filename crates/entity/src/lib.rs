//! 持久化领域模型；类型与行为归入所属聚合，根模块仅组织公开接口。
mod device;
mod location;

pub mod inbox_item;
pub mod inbox_item_file;
pub mod inbox_search_index;
pub mod pair_invite;
pub mod text_delivery;
pub mod transfer_file;
pub mod transfer_session;

pub use device::PeerId;
pub use inbox_item::{Entity as InboxItem, InboxContentKind, InboxSourceKind};
pub use inbox_item_file::Entity as InboxItemFile;
pub use inbox_search_index::Entity as InboxSearchIndex;
pub use location::{CloudObjectRef, CloudProvider, FileLocation, SaveLocation};
pub use pair_invite::Entity as PairInvite;
pub use text_delivery::{
    Entity as TextDelivery, TextDeliveryDirection, TextDeliveryFailure, TextDeliveryStatus,
};
pub use transfer_file::{Entity as TransferFile, FileStatus};
pub use transfer_session::{
    Entity as TransferSession, SessionStatus, SuspendedReason, TerminalReason, TransferDirection,
    TransferPhase,
};
