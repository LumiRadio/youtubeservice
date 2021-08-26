-- Your SQL goes here
CREATE TABLE livechat_messages (
    message_id SERIAl PRIMARY KEY,
    youtube_id VARCHAR NOT NULL UNIQUE,
    channel_id VARCHAR NOT NULL,
    display_name VARCHAR NOT NULL,
    message TEXT NOT NULL,
    sent_at TIMESTAMP NOT NULL,
    received_at TIMESTAMP NOT NULL
)