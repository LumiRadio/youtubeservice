// DO NOT TOUCH THIS FILE!
// THIS FILE IS AUTO-GENERATED BY DIESEL!

table! {
    livechat_messages (message_id) {
        message_id -> Int4,
        youtube_id -> Varchar,
        channel_id -> Varchar,
        display_name -> Varchar,
        message -> Text,
        sent_at -> Timestamp,
        received_at -> Timestamp,
    }
}