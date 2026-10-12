use std::fmt;
use std::ops::BitOr;

use serde::{Deserialize, Serialize};

/// The operations a provider supports, as a bit set.
///
/// A provider advertises these up front; an operation that is advertised but not
/// implemented returns [`crate::Error::NotImplemented`], one that is not
/// advertised returns [`crate::Error::Unsupported`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Capabilities(u32);

impl Capabilities {
    pub const NONE: Self = Self(0);
    pub const WHOAMI: Self = Self(1 << 0);
    pub const LIST_CHATS: Self = Self(1 << 1);
    pub const GET_CHAT: Self = Self(1 << 2);
    pub const LIST_MESSAGES: Self = Self(1 << 3);
    pub const SEARCH_MESSAGES: Self = Self(1 << 4);
    pub const SEND_MESSAGE: Self = Self(1 << 5);
    pub const SEND_MEDIA: Self = Self(1 << 6);
    pub const MARK_READ: Self = Self(1 << 7);
    pub const LIST_MEMBERS: Self = Self(1 << 8);
    pub const SNAPSHOT: Self = Self(1 << 9);
    pub const LISTEN: Self = Self(1 << 10);

    const FLAGS: [(Capabilities, &'static str); 11] = [
        (Capabilities::WHOAMI, "whoami"),
        (Capabilities::LIST_CHATS, "list_chats"),
        (Capabilities::GET_CHAT, "get_chat"),
        (Capabilities::LIST_MESSAGES, "list_messages"),
        (Capabilities::SEARCH_MESSAGES, "search_messages"),
        (Capabilities::SEND_MESSAGE, "send_message"),
        (Capabilities::SEND_MEDIA, "send_media"),
        (Capabilities::MARK_READ, "mark_read"),
        (Capabilities::LIST_MEMBERS, "list_members"),
        (Capabilities::SNAPSHOT, "snapshot"),
        (Capabilities::LISTEN, "listen"),
    ];

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The operation name of a single flag, or `"operation"` for a combination.
    pub fn name(self) -> &'static str {
        for (flag, name) in Self::FLAGS {
            if self.0 == flag.0 {
                return name;
            }
        }
        "operation"
    }

    pub fn names(self) -> Vec<&'static str> {
        Self::FLAGS
            .iter()
            .filter(|(flag, _)| self.contains(*flag))
            .map(|(_, name)| *name)
            .collect()
    }
}

impl BitOr for Capabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl fmt::Debug for Capabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}
