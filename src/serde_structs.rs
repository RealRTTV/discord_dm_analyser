use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeDelta};
use itertools::Itertools;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use serde_this_or_that::as_u64;
use std::ops::Deref;
use fxhash::{FxBuildHasher, FxHashMap};
use parking_lot::RwLock;

pub fn opt_timestamp_from_spec<'de, D: Deserializer<'de>>(deserializer: D) -> anyhow::Result<Option<NaiveDateTime>, D::Error> {
    Ok(match String::deserialize(deserializer) {
        Ok(x) => Some(x.parse::<DateTime<FixedOffset>>().map_err(|_| Error::custom("Could not parse timestamp"))?.with_timezone(&Local).naive_local()),
        Err(_) => None,
    })
}

pub fn timestamp_from_spec<'de, D: Deserializer<'de>>(deserializer: D) -> anyhow::Result<NaiveDateTime, D::Error> {
    opt_timestamp_from_spec(deserializer).map(|x| x.unwrap_or(NaiveDateTime::MIN))
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
            Message::TextMessage(text) => Some(&text.author),
            Message::Call(call) => Some(&call.author),
            Message::PinnedMessage(pin) => Some(&pin.author),
            Message::AddRecipient(add) => Some(&add.author),
            Message::RemoveRecipient(remove) => Some(&remove.author),
            Message::Misc(_) => None
        }).unique().map(|author| author.0.name.as_str()).collect::<Vec<_>>();

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
    #[serde(rename = "35", alias = "20", alias = "23", alias = "ChannelIconChange")]
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

    #[inline]
    pub fn timestamp(&self) -> Option<NaiveDateTime> {
        Some(match self {
            Message::TextMessage(text) => text.timestamp,
            Message::Call(call) => call.start_timestamp,
            Message::PinnedMessage(pin) => pin.timestamp,
            Message::AddRecipient(add) => add.timestamp,
            Message::RemoveRecipient(remove) => remove.timestamp,
            Message::Misc(_) => return None
        })
    }

    #[inline]
    pub fn author(&self) -> Option<&Author> {
        Some(match self {
            Message::TextMessage(text) => &text.author,
            Message::Call(call) => &call.author,
            Message::PinnedMessage(pin) => &pin.author,
            Message::AddRecipient(add) => &add.author,
            Message::RemoveRecipient(remove) => &remove.author,
            Message::Misc(_) => return None
        })
    }

    #[inline]
    pub fn id(&self) -> Option<u64> {
        Some(match self {
            Message::TextMessage(text) => text.id,
            Message::Call(call) => call.id,
            _ => return None
        })
    }
}

#[derive(Deserialize)]
pub struct TextMessage {
    #[serde(deserialize_with = "as_u64")]
    pub id: u64,
    pub content: String,
    pub author: AuthorReference,
    #[serde(deserialize_with = "timestamp_from_spec")]
    pub timestamp: NaiveDateTime,
    #[serde(deserialize_with = "opt_timestamp_from_spec", rename = "timestampEdited")]
    pub edited_timestamp: Option<NaiveDateTime>,
    pub attachments: Vec<Attachment>,
    pub reference: Option<Reference>,
}

impl TextMessage {
    pub fn content_alphanumeric_lowercase(&self) -> String {
        self.content.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace()).collect::<String>()
    }
}

#[derive(Eq, PartialEq, Hash)]
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

#[derive(Eq, PartialEq, Hash)]
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
    pub start_timestamp: NaiveDateTime,
    #[serde(rename = "callEndedTimestamp", deserialize_with = "timestamp_from_spec")]
    pub end_timestamp: NaiveDateTime,
    pub author: AuthorReference,
}

impl Call {
    pub fn duration(&self) -> TimeDelta {
        (self.end_timestamp - self.start_timestamp).max(TimeDelta::zero())
    }
}

#[derive(Deserialize)]
pub struct PinnedMessage {
    #[serde(deserialize_with = "timestamp_from_spec")]
    pub timestamp: NaiveDateTime,
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
    pub timestamp: NaiveDateTime,
    pub author: AuthorReference,
    #[serde(rename = "mentions")]
    pub added: Vec<AuthorReference>,
}

#[derive(Deserialize)]
pub struct RemoveRecipient {
    #[serde(deserialize_with = "timestamp_from_spec")]
    pub timestamp: NaiveDateTime,
    pub author: AuthorReference,
    #[serde(rename = "mentions")]
    pub removed: Vec<AuthorReference>,
}
