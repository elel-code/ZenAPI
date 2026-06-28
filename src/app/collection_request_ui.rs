mod actions;
mod editor;

pub(super) use self::actions::wire_collection_request_actions;
#[cfg(test)]
pub(super) use self::editor::{collection_body_to_slint, collection_request_from_editor};
