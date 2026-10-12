use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The messenger platforms hyper-messenger can talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ProviderKind {
    KakaoTalk,
    Discord,
    Telegram,
    ChannelTalk,
    Mock,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 5] = [
        ProviderKind::KakaoTalk,
        ProviderKind::Discord,
        ProviderKind::Telegram,
        ProviderKind::ChannelTalk,
        ProviderKind::Mock,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ProviderKind::KakaoTalk => "kakaotalk",
            ProviderKind::Discord => "discord",
            ProviderKind::Telegram => "telegram",
            ProviderKind::ChannelTalk => "channeltalk",
            ProviderKind::Mock => "mock",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            ProviderKind::KakaoTalk => "KakaoTalk",
            ProviderKind::Discord => "Discord",
            ProviderKind::Telegram => "Telegram",
            ProviderKind::ChannelTalk => "Channel Talk",
            ProviderKind::Mock => "Mock",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            ProviderKind::KakaoTalk => {
                "KakaoTalk, driven from the local desktop app's session and database"
            }
            ProviderKind::Discord => "Discord, as your user account or a bot token",
            ProviderKind::Telegram => "Telegram, as your user account or a bot token",
            ProviderKind::ChannelTalk => "Channel Talk, as an operator or with API credentials",
            ProviderKind::Mock => "An in-memory provider used for development and tests",
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProviderKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value
            .trim()
            .to_ascii_lowercase()
            .replace(['-', '_', ' '], "");
        let kind = match normalized.as_str() {
            "kakaotalk" | "kakao" => ProviderKind::KakaoTalk,
            "discord" => ProviderKind::Discord,
            "telegram" => ProviderKind::Telegram,
            "channeltalk" | "channel" | "channeltalkbot" => ProviderKind::ChannelTalk,
            "mock" | "memory" => ProviderKind::Mock,
            _ => {
                return Err(Error::InvalidInput {
                    message: format!("unknown provider `{value}`"),
                });
            }
        };
        Ok(kind)
    }
}
