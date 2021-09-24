use crate::YouTubeChatMessage;
use chrono::NaiveDateTime;
use diesel::Queryable;
use std::convert::TryInto;

use super::schema::livechat_messages;

#[derive(Queryable)]
pub struct LivechatMessage {
    pub message_id: i32,
    pub youtube_id: String,
    pub channel_id: String,
    pub display_name: String,
    pub message: String,
    pub sent_at: NaiveDateTime,
    pub received_at: NaiveDateTime,
}

#[derive(Insertable)]
#[table_name = "livechat_messages"]
pub struct InsertLivechatMessage {
    pub channel_id: String,
    pub youtube_id: String,
    pub display_name: String,
    pub message: String,
    pub sent_at: NaiveDateTime,
    pub received_at: NaiveDateTime,
}

impl From<YouTubeChatMessage> for InsertLivechatMessage {
    fn from(msg: YouTubeChatMessage) -> Self {
        let sent_at_ts = msg.sent_at_timestamp.unwrap();
        let sent_at_chrono =
            NaiveDateTime::from_timestamp(sent_at_ts.seconds, sent_at_ts.nanos.try_into().unwrap());
        let received_at_ts = msg.received_at_timestamp.unwrap();
        let received_at_chrono = NaiveDateTime::from_timestamp(
            received_at_ts.seconds,
            received_at_ts.nanos.try_into().unwrap(),
        );
        InsertLivechatMessage {
            channel_id: msg.channel_id,
            display_name: msg.display_name,
            message: msg.message,
            sent_at: sent_at_chrono,
            received_at: received_at_chrono,
            youtube_id: msg.message_id,
        }
    }
}

impl From<&YouTubeChatMessage> for InsertLivechatMessage {
    fn from(msg: &YouTubeChatMessage) -> Self {
        let sent_at_ts = msg.sent_at_timestamp.as_ref().unwrap();
        let sent_at_chrono =
            NaiveDateTime::from_timestamp(sent_at_ts.seconds, sent_at_ts.nanos.try_into().unwrap());
        let received_at_ts = msg.received_at_timestamp.as_ref().unwrap();
        let received_at_chrono = NaiveDateTime::from_timestamp(
            received_at_ts.seconds,
            received_at_ts.nanos.try_into().unwrap(),
        );
        InsertLivechatMessage {
            channel_id: msg.channel_id.clone(),
            display_name: msg.display_name.clone(),
            message: msg.message.clone(),
            sent_at: sent_at_chrono,
            received_at: received_at_chrono,
            youtube_id: msg.message_id.clone(),
        }
    }
}
