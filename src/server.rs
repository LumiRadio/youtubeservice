#[macro_use]
extern crate diesel;

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ::log::{debug, error, info};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::ConnectionManager;
use google_youtube3::api::{LiveChatMessage, LiveChatMessageSnippet, LiveChatTextMessageDetails};
use google_youtube3::YouTube;
use models::InsertLivechatMessage;
use prost_types::Timestamp;
use r2d2::Pool;
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Response, Status};

mod log;
mod models;
mod schema;
mod youtube;

pub mod youtube_service {
    use crate::models::LivechatMessage;
    use prost_types::Timestamp;

    tonic::include_proto!("youtubeservice");

    impl From<LivechatMessage> for YouTubeChatMessage {
        fn from(msg: LivechatMessage) -> Self {
            let sent_at_timestamp = Timestamp {
                seconds: msg.sent_at.timestamp() as i64,
                nanos: msg.sent_at.timestamp_subsec_nanos() as i32,
            };
            let received_at_timestamp = Timestamp {
                seconds: msg.received_at.timestamp() as i64,
                nanos: msg.received_at.timestamp_subsec_nanos() as i32,
            };
            return YouTubeChatMessage {
                channel_id: msg.channel_id,
                display_name: msg.display_name,
                message: msg.message,
                sent_at_timestamp: Some(sent_at_timestamp),
                received_at_timestamp: Some(received_at_timestamp),
                message_id: msg.youtube_id,
            };
        }
    }

    impl From<&LivechatMessage> for YouTubeChatMessage {
        fn from(msg: &LivechatMessage) -> Self {
            let sent_at_timestamp = Timestamp {
                seconds: msg.sent_at.timestamp() as i64,
                nanos: msg.sent_at.timestamp_subsec_nanos() as i32,
            };
            let received_at_timestamp = Timestamp {
                seconds: msg.received_at.timestamp() as i64,
                nanos: msg.received_at.timestamp_subsec_nanos() as i32,
            };
            return YouTubeChatMessage {
                channel_id: msg.channel_id.clone(),
                display_name: msg.display_name.clone(),
                message: msg.message.clone(),
                sent_at_timestamp: Some(sent_at_timestamp),
                received_at_timestamp: Some(received_at_timestamp),
                message_id: msg.youtube_id.clone(),
            };
        }
    }

    impl From<Vec<YouTubeChatMessage>> for YouTubeChatMessages {
        fn from(msgs: Vec<YouTubeChatMessage>) -> Self {
            YouTubeChatMessages { messages: msgs }
        }
    }
}

use youtube_service::you_tube_service_server::{YouTubeService, YouTubeServiceServer};
use youtube_service::YouTubeChatMessage;

use crate::log::{log_google_errors, setup_log};
use crate::models::LivechatMessage;
use crate::youtube::{authenticate_google, body_to_string, get_livechat_id};

pub struct YouTubeServiceImpl {
    messages_tx: Sender<YouTubeChatMessage>,
    youtube_hub: Arc<YouTube>,
    livechat_id: String,
    database_connection: Pool<ConnectionManager<PgConnection>>,
}

impl YouTubeServiceImpl {
    pub fn new(
        tx: Sender<YouTubeChatMessage>,
        youtube_hub: Arc<YouTube>,
        livechat_id: String,
        database_connection: Pool<ConnectionManager<PgConnection>>,
    ) -> Self {
        YouTubeServiceImpl {
            messages_tx: tx,
            youtube_hub,
            livechat_id,
            database_connection,
        }
    }
}

#[tonic::async_trait]
impl YouTubeService for YouTubeServiceImpl {
    async fn send_message(
        &self,
        request: tonic::Request<String>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        // Build a livechat message
        let message = request.into_inner();
        let mut livechat_message = LiveChatMessage::default();
        let mut livechat_snippet = LiveChatMessageSnippet::default();
        let mut text_message_details = LiveChatTextMessageDetails::default();
        livechat_snippet.type_ = Some("textMessageEvent".to_string());
        livechat_snippet.live_chat_id = Some(self.livechat_id.clone());
        text_message_details.message_text = Some(message);
        livechat_snippet.text_message_details = Some(text_message_details);
        livechat_message.snippet = Some(livechat_snippet);

        // Send the message to the YouTube API
        let response_result = self
            .youtube_hub
            .live_chat_messages()
            .insert(livechat_message)
            .add_part("snippet")
            .doit()
            .await;
        // If there was an error, log it and return the error to the client
        if let Err(e) = response_result {
            let error_message = log_google_errors(e).await;
            return Err(Status::new(
                tonic::Code::Internal,
                format!("{}", error_message),
            ));
        }
        return Ok(Response::new(()));
    }

    type SubscribeMessagesStream = ReceiverStream<Result<YouTubeChatMessage, Status>>;

    async fn subscribe_messages(
        &self,
        _: tonic::Request<()>,
    ) -> Result<tonic::Response<Self::SubscribeMessagesStream>, tonic::Status> {
        // Create a pair of mpsc channels to send messages to the client
        let (tx, rx) = mpsc::channel(4);
        // Create a receiver for the broadcast stream because we have a new listener
        let mut message_rx = self.messages_tx.subscribe();

        // Spawn a future that will forward the messages from the broadcast channel to the mpsc channel
        tokio::spawn(async move {
            while let Ok(message) = message_rx.recv().await {
                if tx.is_closed() {
                    println!("Ending channel...");
                    break;
                }

                if let Err(e) = tx.send(Ok(message)).await {
                    println!("Error sending message: {}", e);
                }
            }
        });

        // Return the channel that will receive the messages
        return Ok(Response::new(ReceiverStream::new(rx)));
    }

    async fn get_messages(
        &self,
        request: tonic::Request<youtube_service::GetMessageRequest>,
    ) -> Result<tonic::Response<youtube_service::YouTubeChatMessages>, tonic::Status> {
        let get_message_request = request.into_inner();
        use crate::schema::livechat_messages::dsl::*;

        // Get messages from the database
        let db_conn = &self.database_connection.get().unwrap();
        let results = livechat_messages
            .order(sent_at.desc())
            .limit(get_message_request.limit.into())
            .offset(get_message_request.offset.into())
            .load::<LivechatMessage>(db_conn)
            .unwrap();
        let converted: Vec<YouTubeChatMessage> = results.iter().map(|m| m.into()).collect();
        return Ok(Response::new(converted.into()));
    }
}

pub fn insert_chat_message(
    database_connection: &Pool<ConnectionManager<PgConnection>>,
    chat_message: &YouTubeChatMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if the message already exists
    // If it does, do not insert it again
    use diesel::dsl::exists;
    use diesel::select;
    use schema::livechat_messages::dsl::{livechat_messages, youtube_id};
    let exists: bool = select(exists(
        livechat_messages.filter(youtube_id.eq(chat_message.message_id.clone())),
    ))
    .get_result(&database_connection.get()?)?;
    if exists {
        debug!(
            "Skipping message with id {} because it already exists",
            chat_message.message_id
        );
        return Ok(());
    }

    // Insert the message
    let insert_message = InsertLivechatMessage::from(chat_message);
    diesel::insert_into(schema::livechat_messages::table)
        .values(insert_message)
        .execute(&database_connection.get()?)?;
    return Ok(());
}

async fn fetch_messages(
    bot_hub: &YouTube,
    streamer_hub: &YouTube,
    livechat_id: String,
    tx: Sender<YouTubeChatMessage>,
    pool: &Pool<ConnectionManager<PgConnection>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Clone the livechat id so we can change it later
    let mut livechat_id_clone = livechat_id.clone();
    // Initialize the page token to be empty
    let mut page_token: Option<String> = None;
    // Loop forever or until the future is cancelled
    loop {
        // Prepare the query to the YouTube API
        let mut prepare_livechat = bot_hub.live_chat_messages().list(
            livechat_id_clone.as_str(),
            &vec!["snippet".to_string(), "authorDetails".to_string()],
        );
        // If we have a page token, add it to the query
        if page_token.is_some() {
            prepare_livechat = prepare_livechat.page_token(page_token.as_ref().unwrap().as_str());
        }
        // Execute the query
        let response_result = prepare_livechat.doit().await;
        // If the query failed, spit out an error and continue
        if let Err(e) = response_result {
            error!("Error while fetching chat messages: {}", e);
            log_google_errors(e).await;

            // Try to get the latest livechat id
            info!("Trying to recover by receiving the latest livechat id");
            let livechat_id_result = get_livechat_id(streamer_hub).await;
            if livechat_id_result.is_none() {
                // If we can't get the latest livechat id, wait for 10 seconds and try again
                error!("Unable to get livechat ID! Retrying in 10 seconds...");
                tokio::time::sleep(Duration::from_secs(10)).await;
            } else {
                livechat_id_clone = livechat_id_result.unwrap().clone();
            }
            continue;
        }
        // Read the response
        let (response_body, response) = response_result.expect("response_result");
        let body_string = body_to_string(response_body).await;
        let items = response.items;
        if items.is_none() {
            error!(
                "Error while fetching chat messages: Items is none! Response: {}",
                body_string
            );

            // Try to get the latest livechat id
            info!("Trying to recover by receiving the latest livechat id");
            let livechat_id_result = get_livechat_id(streamer_hub).await;
            if livechat_id_result.is_none() {
                // If we can't get the latest livechat id, wait for 10 seconds and try again
                error!("Unable to get livechat ID! Retrying in 10 seconds...");
                tokio::time::sleep(Duration::from_secs(10)).await;
            } else {
                livechat_id_clone = livechat_id_result.unwrap().clone();
            }

            continue;
        }
        page_token = response.next_page_token;
        let wait_for_millis = response.polling_interval_millis.unwrap();
        let items = items.unwrap();
        // For each message in the response, send it to the broadcast channel
        for msg in items {
            let author_details = msg.author_details.unwrap();
            let channel_id = author_details.channel_id.unwrap();
            let display_name = author_details.display_name.unwrap();
            let message_snippet = msg.snippet.unwrap();
            let message_type = message_snippet.type_.unwrap();
            let published_at = message_snippet.published_at.unwrap();
            let sent_at = chrono::DateTime::parse_from_rfc3339(published_at.as_str()).unwrap();
            let received_at = chrono::Utc::now();
            let sent_at_timestamp = Timestamp {
                seconds: sent_at.timestamp() as i64,
                nanos: sent_at.timestamp_subsec_nanos() as i32,
            };
            let received_at_timestamp = Timestamp {
                seconds: received_at.timestamp() as i64,
                nanos: received_at.timestamp_subsec_nanos() as i32,
            };
            let message_id = msg.id.unwrap();

            // Match the message type to cover more than just chat messages
            match message_type.as_str() {
                "textMessageEvent" => {
                    // Create a chat message object, insert it into the database and send it to the broadcast channel
                    let message_text = message_snippet.display_message.unwrap();
                    info!("{} >> {}", display_name, message_text);
                    let chat_message = YouTubeChatMessage {
                        channel_id,
                        display_name,
                        message: message_text,
                        sent_at_timestamp: Some(sent_at_timestamp),
                        received_at_timestamp: Some(received_at_timestamp),
                        message_id,
                    };
                    let insert_result = insert_chat_message(&pool, &chat_message);
                    if let Err(e) = insert_result {
                        error!("Error while inserting chat message: {}", e);
                    }
                    tx.send(chat_message)?;
                }
                _ => {}
            }
        }

        // Wait for the amount of time specified by the API before requesting again
        tokio::time::sleep(Duration::from_millis(wait_for_millis.into())).await;
    }
}

pub fn connect_to_database() -> Pool<ConnectionManager<PgConnection>> {
    // Get the database URL from the environment
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = ConnectionManager::new(database_url);
    // Create a connection pool of 10 connections
    let pool = Pool::builder().max_size(10).build(manager).unwrap();
    return pool;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file if present
    dotenv::dotenv().ok();

    // Setup global logging handler
    // This outputs to stdout and syslog
    setup_log(env::var_os("DEBUG").is_some());
    debug!("Debug mode activated!");

    // Connect to database and create a connection pool
    let db_connection = connect_to_database();

    // Get the address and port to use for the gRPC server from the environment variables
    let env_addr = env::var_os("YTS_GRPC_ADDRESS");
    let mut addr: SocketAddr = "0.0.0.0:50051".parse()?;
    if let Some(osstr_addr) = env_addr {
        let str_addr = osstr_addr.into_string().unwrap();
        addr = str_addr.parse()?;
    }
    // Create 2 hubs for the YouTube API (authenticates automatically with the youtube scope)
    let (bot_hub, streamer_hub) = authenticate_google().await?;
    // Wrap the hub in an atomic reference counter to share it safetly across threads
    let bot_hub_arc = Arc::new(bot_hub);
    let streamer_hub_arc = Arc::new(streamer_hub);

    // Get livechat id from either the environment variable or the currently running broadcast, if there is one
    // If there is no currently running broadcast, the program will try again until it finds one
    // Until then, the program will halt and the gRPC server will not run
    let livechat_id: String;
    let mut livechat_id_opt: Option<String> = None;
    while livechat_id_opt.is_none() {
        livechat_id_opt = if env::var("YTS_LIVECHAT_ID").is_ok() {
            Some(env::var("YTS_LIVECHAT_ID").unwrap())
        } else {
            get_livechat_id(&streamer_hub_arc).await
        };
        if livechat_id_opt.is_none() {
            error!("Unable to determine livechat ID, retrying in 30 seconds");
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }
    livechat_id = livechat_id_opt.unwrap();

    info!("Livechat ID determined: {}", livechat_id);

    // Create a broadcast channel to send messages across futures
    let (tx, _) = tokio::sync::broadcast::channel(100);
    // Create a service implementation
    let service = YouTubeServiceImpl::new(
        tx.clone(),
        bot_hub_arc.clone(),
        livechat_id.clone(),
        db_connection.clone(),
    );

    // Spawn the gRPC server future with our service implementation as well as our fetch function future
    let (_, _) = tokio::join!(
        Server::builder()
            .add_service(YouTubeServiceServer::new(service))
            .serve(addr),
        fetch_messages(&bot_hub_arc, &streamer_hub_arc, livechat_id, tx, &db_connection)
    );

    return Ok(());
}
