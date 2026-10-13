use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The messenger platforms horon-messenger can talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ProviderKind {
    KakaoTalk,
    Discord,
    Telegram,
    ChannelTalk,
    Mock,
    Slack,
    SlackBot,
    TelegramBot,
    WhatsApp,
    WhatsAppBot,
    Line,
    IMessage,
    Instagram,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 13] = [
        ProviderKind::KakaoTalk,
        ProviderKind::Discord,
        ProviderKind::Telegram,
        ProviderKind::ChannelTalk,
        ProviderKind::Mock,
        ProviderKind::Slack,
        ProviderKind::SlackBot,
        ProviderKind::TelegramBot,
        ProviderKind::WhatsApp,
        ProviderKind::WhatsAppBot,
        ProviderKind::Line,
        ProviderKind::IMessage,
        ProviderKind::Instagram,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            ProviderKind::KakaoTalk => "kakaotalk",
            ProviderKind::Discord => "discord",
            ProviderKind::Telegram => "telegram",
            ProviderKind::ChannelTalk => "channeltalk",
            ProviderKind::Mock => "mock",
            ProviderKind::Slack => "slack",
            ProviderKind::SlackBot => "slackbot",
            ProviderKind::TelegramBot => "telegrambot",
            ProviderKind::WhatsApp => "whatsapp",
            ProviderKind::WhatsAppBot => "whatsappbot",
            ProviderKind::Line => "line",
            ProviderKind::IMessage => "imessage",
            ProviderKind::Instagram => "instagram",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            ProviderKind::KakaoTalk => "KakaoTalk",
            ProviderKind::Discord => "Discord",
            ProviderKind::Telegram => "Telegram",
            ProviderKind::ChannelTalk => "Channel Talk",
            ProviderKind::Mock => "Mock",
            ProviderKind::Slack => "Slack",
            ProviderKind::SlackBot => "Slack Bot",
            ProviderKind::TelegramBot => "Telegram Bot",
            ProviderKind::WhatsApp => "WhatsApp",
            ProviderKind::WhatsAppBot => "WhatsApp Bot",
            ProviderKind::Line => "LINE",
            ProviderKind::IMessage => "iMessage",
            ProviderKind::Instagram => "Instagram",
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
            ProviderKind::Slack => "Slack, as your user account or a bot token",
            ProviderKind::SlackBot => "Slack, as a bot user",
            ProviderKind::TelegramBot => "Telegram, as a bot token",
            ProviderKind::WhatsApp => "WhatsApp, as your user account",
            ProviderKind::WhatsAppBot => "WhatsApp, as a bot number",
            ProviderKind::Line => "LINE, as your user account",
            ProviderKind::IMessage => "iMessage, driven from the local Messages app",
            ProviderKind::Instagram => "Instagram, as your user account",
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
            "slack" => ProviderKind::Slack,
            "slackbot" => ProviderKind::SlackBot,
            "telegrambot" => ProviderKind::TelegramBot,
            "whatsapp" => ProviderKind::WhatsApp,
            "whatsappbot" => ProviderKind::WhatsAppBot,
            "line" => ProviderKind::Line,
            "imessage" => ProviderKind::IMessage,
            "instagram" | "ig" => ProviderKind::Instagram,
            _ => {
                return Err(Error::InvalidInput {
                    message: format!("unknown provider `{value}`"),
                });
            }
        };
        Ok(kind)
    }
}
