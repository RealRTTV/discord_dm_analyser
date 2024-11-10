use std::num::Add;
use chrono::{DateTime, FixedOffset};
use itertools::Itertools;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use serde_this_or_that::as_u64;
use std::ops::Deref;
use fxhash::{FxBuildHasher, FxHashMap};
use parking_lot::RwLock;

pub fn timestamp_from_spec<'de, D: Deserializer<'de>>(deserializer: D) -> anyhow::Result<u64, D::Error> {
    Ok(match String::deserialize(deserializer) {
        Ok(x) => x.parse::<DateTime<FixedOffset>>().map_err(|_| Error::custom("Could not parse timestamp"))?.timestamp_millis() as u64,
        Err(_) => 0,
    })
}

#[derive(Deserialize)]
pub struct UninitDirectMessages {
    channel: ChannelInfo,
    messages: Vec<Message>,
}

impl TryInto<DirectMessages> for UninitDirectMessages {
    type Error = anyhow::Error;

    fn try_into(self) -> anyhow::Result<DirectMessages> {
        let Self { channel, messages } = self;

        let mut dms = DirectMessages {
            channel,
            messages,
        };

        dms.init()?;

        Ok(dms)
    }
}

pub struct DirectMessages {
    pub channel: ChannelInfo,
    pub messages: Vec<Message>,
}

impl DirectMessages {
    fn init(&mut self) -> anyhow::Result<()> {
        self.channel.authors = self.messages.iter().filter_map(|message| match message {
            Message::TextMessage(text) => Some(&text.author.name),
            Message::Call(call) => Some(&call.author.name),
            Message::PinnedMessage(pin) => Some(&pin.author.name),
            Message::Misc(_) => None
        }).unique().map(|s| s.as_str()).collect::<Vec<_>>();

        Ok(())
    }
}

#[derive(Deserialize)]
pub struct ChannelInfo {
    #[serde(deserialize_with = "as_u64")]
    pub id: u64,
    pub name: String,
    #[serde(default, skip)]
    pub authors: Vec<&'static str>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "Default", alias = "Reply")]
    TextMessage(TextMessage),
    #[serde(rename = "Call")]
    Call(Call),
    #[serde(rename = "ChannelPinnedMessage")]
    PinnedMessage(PinnedMessage),
    #[serde(rename = "RecipientAdd")]
    AddRecipient(AddRecipient),
    #[serde(rename = "RecipientRemove")]
    RemoveRecipient(RemoveRecipient),
    #[serde(rename = "35", alias = "20", alias = "ChannelIconChange")]
    Misc(Value),
}

impl Message {
    #[inline]
    pub fn as_text_message(&self) -> Option<&TextMessage> {
        if let Message::TextMessage(inner) = self {
            Some(inner)
        } else {
            None
        }
    }

    #[inline]
    pub fn as_call(&self) -> Option<&Call> {
        if let Message::Call(inner) = self {
            Some(inner)
        } else {
            None
        }
    }

    #[inline]
    pub fn as_pinned_message(&self) -> Option<&PinnedMessage> {
        if let Message::PinnedMessage(inner) = self {
            Some(inner)
        } else {
            None
        }
    }

    #[inline]
    pub fn as_add_recipient(&self) -> Option<&AddRecipient> {
        if let Message::AddRecipient(inner) = self {
            Some(inner)
        } else {
            None
        }
    }

    #[inline]
    pub fn as_remove_recipient(&self) -> Option<&RemoveRecipient> {
        if let Message::RemoveRecipient(inner) = self {
            Some(inner)
        } else {
            None
        }
    }

    #[inline]
    pub fn as_misc(&self) -> Option<&Value> {
        if let Message::Misc(inner) = self {
            Some(inner)
        } else {
            None
        }
    }
}

#[derive(Deserialize)]
pub struct TextMessage {
    #[serde(deserialize_with = "as_u64")]
    pub id: u64,
    pub content: String,
    pub author: AuthorReference,
    #[serde(deserialize_with = "timestamp_from_spec")]
    pub timestamp: u64,
    pub attachments: Vec<Attachment>,
    pub reference: Option<Reference>,
}

impl TextMessage {
    pub fn content_alphanumeric_lowercase(&self) -> String {
        self.content.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace()).collect::<String>()
    }
}

pub struct AuthorReference(&'static Author);

impl<'de> Deserialize<'de> for AuthorReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>
    {
        static EXISTING_AUTHORS: RwLock<FxHashMap<u64, &'static Author>> = RwLock::new(FxHashMap::with_hasher(FxBuildHasher::new()));

        Ok(match DeserializedAuthor::deserialize(deserializer) {
            Ok(author) => {
                let DeserializedAuthor { id, nickname, name } = author;
                let author = Author { id, nickname, name };
                let read = EXISTING_AUTHORS.read();
                if let Some(author) = read.get(&author.id) {
                    Self(*author)
                } else {
                    drop(read);
                    let author = Box::leak(Box::new(author));
                    let mut write = EXISTING_AUTHORS.write();
                    write.insert(author.id, author);
                    Self(author)
                }
            },
            Err(e) => return Err(e)
        })
    }
}

impl Deref for AuthorReference {
    type Target = Author;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Author {
    pub id: u64,
    pub nickname: String,
    pub name: String,
}

#[derive(Deserialize)]
struct DeserializedAuthor {
    #[serde(deserialize_with = "as_u64")]
    id: u64,
    nickname: String,
    name: String,
}

#[derive(Deserialize)]
pub struct Attachment {
    #[serde(deserialize_with = "as_u64")]
    pub id: u64,
    pub url: String,
    #[serde(rename = "fileName")]
    pub name: String,
    #[serde(rename = "fileSizeBytes")]
    pub size: usize,
}

#[derive(Deserialize)]
pub struct Call {
    #[serde(deserialize_with = "as_u64")]
    pub id: u64,
    #[serde(rename = "timestamp", deserialize_with = "timestamp_from_spec")]
    pub start_timestamp: u64,
    #[serde(rename = "callEndedTimestamp", deserialize_with = "timestamp_from_spec")]
    pub end_timestamp: u64,
    pub author: AuthorReference,
}

impl Call {
    pub fn duration(&self) -> u64 {
        self.end_timestamp.saturating_sub(self.start_timestamp)
    }
}

#[derive(Deserialize)]
pub struct PinnedMessage {
    #[serde(deserialize_with = "timestamp_from_spec")]
    pub timestamp: u64,
    pub author: AuthorReference,
    reference: Reference,
}

impl Deref for PinnedMessage {
    type Target = Reference;

    fn deref(&self) -> &Self::Target {
        &self.reference
    }
}

#[derive(Deserialize)]
pub struct Reference {
    #[serde(rename = "messageId", deserialize_with = "as_u64")]
    reference_message_id: u64,
}

#[derive(Deserialize)]
pub struct AddRecipient {
    #[serde(deserialize_with = "timestamp_from_spec")]
    pub timestamp: u64,
    pub author: AuthorReference,
    #[serde(rename = "mentions")]
    pub added: Vec<AuthorReference>,
}

#[derive(Deserialize)]
pub struct RemoveRecipient {
    #[serde(deserialize_with = "timestamp_from_spec")]
    pub timestamp: u64,
    pub author: AuthorReference,
    #[serde(rename = "mentions")]
    pub removed: Vec<AuthorReference>,
}