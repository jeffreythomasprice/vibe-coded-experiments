//! Lightweight enums that let the shared tag-editor and confirm-delete
//! screens serve both conversations and documents without branching in the
//! outer event loop.

use crate::protocol::{Request, Response};

use super::{Screen, Transition};

/// Entity whose tags the tag editor is editing.
#[derive(Clone, Debug)]
pub enum TagTarget {
    Conversation(String),
    Document(i64),
}

impl TagTarget {
    /// Request that loads the current tags for this target.
    pub fn tags_request(&self) -> Request {
        match self {
            TagTarget::Conversation(id) => Request::ConversationTags { id: id.clone() },
            TagTarget::Document(id) => Request::DocumentTags { id: *id },
        }
    }

    pub fn add_tag_request(&self, tag: String) -> Request {
        match self {
            TagTarget::Conversation(id) => Request::ConversationAddTag {
                id: id.clone(),
                tag,
            },
            TagTarget::Document(id) => Request::DocumentAddTag { id: *id, tag },
        }
    }

    pub fn remove_tag_request(&self, tag: String) -> Request {
        match self {
            TagTarget::Conversation(id) => Request::ConversationRemoveTag {
                id: id.clone(),
                tag,
            },
            TagTarget::Document(id) => Request::DocumentRemoveTag { id: *id, tag },
        }
    }

    /// Pull the tag list out of the target-appropriate response variant.
    /// Returns `None` if the frame doesn't match this target.
    pub fn extract_tags(&self, response: &Response) -> Option<Vec<String>> {
        match (self, response) {
            (TagTarget::Conversation(_), Response::ConversationTags { tags })
            | (TagTarget::Document(_), Response::DocumentTags { tags }) => Some(tags.clone()),
            _ => None,
        }
    }

    /// Screen to return to when the user presses Esc.
    pub fn back_transition(&self) -> Transition {
        match self {
            TagTarget::Conversation(_) => Transition::To(Screen::ConversationList(
                super::ConversationListState::new_loading(),
            )),
            TagTarget::Document(_) => Transition::To(Screen::DocumentList(Box::default())),
        }
    }
}

/// Entity the confirm-delete dialog is about to delete.
#[derive(Clone, Debug)]
pub enum DeleteTarget {
    Conversation(String),
    Document(i64),
}

impl DeleteTarget {
    pub fn delete_request(&self) -> Request {
        match self {
            DeleteTarget::Conversation(id) => Request::ConversationDelete { id: id.clone() },
            DeleteTarget::Document(id) => Request::DocumentDelete { id: *id },
        }
    }

    pub fn back_transition(&self) -> Transition {
        match self {
            DeleteTarget::Conversation(_) => Transition::To(Screen::ConversationList(
                super::ConversationListState::new_loading(),
            )),
            DeleteTarget::Document(_) => Transition::To(Screen::DocumentList(Box::default())),
        }
    }

    /// Human-readable kind for the dialog ("conversation" / "document").
    pub fn kind(&self) -> &'static str {
        match self {
            DeleteTarget::Conversation(_) => "conversation",
            DeleteTarget::Document(_) => "document",
        }
    }
}
