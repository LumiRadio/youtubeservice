use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use google_youtube3::YouTube;
use google_youtube3::api::{LiveChatMessage, LiveChatMessageSnippet, LiveChatTextMessageDetails};
use ::log::{debug, error, info};
use prost_types::Timestamp;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::{mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};

mod youtube;
mod log;

pub mod youtube_service {
    tonic::include_proto!("youtubeservice");
}

use youtube_service::you_tube_service_server::{YouTubeService, YouTubeServiceServer};
use youtube_service::{
    GetMessageRequest, YouTubeChatMessage, YouTubeChatMessages,
};
use yup_oauth2::{DeviceFlowAuthenticator, InstalledFlowAuthenticator};

use crate::log::{log_google_errors, setup_log};
use crate::youtube::{authenticate_google, body_to_string, get_livechat_id};

pub struct YouTubeServiceImpl {
    messages_tx: Sender<YouTubeChatMessage>,
    youtube_hub: Arc<YouTube>,
    livechat_id: String,
}

impl YouTubeServiceImpl {
    pub fn new(tx: Sender<YouTubeChatMessage>, youtube_hub: Arc<YouTube>, livechat_id: String) -> Self {
        YouTubeServiceImpl { messages_tx: tx, youtube_hub, livechat_id }
    }
}

#[tonic::async_trait]
impl YouTubeService for YouTubeServiceImpl {
    async fn send_message(
        &self,
        request: tonic::Request<String>,
    ) -> Result<tonic::Response<()>, tonic::Status> {
        let message = request.into_inner();
        let mut livechat_message = LiveChatMessage::default();
        let mut livechat_snippet = LiveChatMessageSnippet::default();
        let mut text_message_details = LiveChatTextMessageDetails::default();
        livechat_snippet.type_ = Some("textMessageEvent".to_string());
        livechat_snippet.live_chat_id = Some(self.livechat_id.clone());
        text_message_details.message_text = Some(message);
        livechat_snippet.text_message_details = Some(text_message_details);
        livechat_message.snippet = Some(livechat_snippet);

        let response_result = self.youtube_hub.live_chat_messages()
            .insert(livechat_message)
            .add_part("snippet")
            .doit().await;
        if let Err(e) = response_result {
            let error_message = log_google_errors(e).await;
            return Err(Status::new(tonic::Code::Internal, format!("{}", error_message)));
        }
        return Ok(Response::new(()));
    }

    type SubscribeMessagesStream = ReceiverStream<Result<YouTubeChatMessage, Status>>;

    async fn subscribe_messages(
        &self,
        request: tonic::Request<()>,
    ) -> Result<tonic::Response<Self::SubscribeMessagesStream>, tonic::Status> {
        let (mut tx, rx) = mpsc::channel(4);
        let mut message_rx = self.messages_tx.subscribe();
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

        return Ok(Response::new(ReceiverStream::new(rx)));
    }

    async fn get_messages(
        &self,
        request: tonic::Request<youtube_service::GetMessageRequest>,
    ) -> Result<tonic::Response<youtube_service::YouTubeChatMessages>, tonic::Status> {
        todo!()
    }

    async fn authenticate(&self, request: tonic::Request<String>) -> Result<tonic::Response<bool>, tonic::Status> {
        todo!()
    }
}

async fn fetch_messages(hub: &YouTube, livechat_id: String, tx: Sender<YouTubeChatMessage>) -> Result<(), Box<dyn std::error::Error>> {
    let mut page_token: Option<String> = None;
    loop {
        let mut prepare_livechat = hub.live_chat_messages()
            .list(livechat_id.as_str(), &vec!["snippet".to_string(), "authorDetails".to_string()]);
        if page_token.is_some() {
            prepare_livechat = prepare_livechat.page_token(page_token.as_ref().unwrap().as_str());
        }
        let response_result = prepare_livechat.doit().await;
        if let Err(e) = response_result {
            error!("Error while fetching chat messages: {}", e);
            match e {
                google_youtube3::Error::BadRequest(error_response) => {
                    error!("Response: {}", error_response.error.message);
                    for server_error in &error_response.error.errors {
                        error!("{}", server_error.message);
                    }
                },
                _ => {}
            }
            continue;
        }
        let (response_body, response) = response_result.expect("response_result");
        let body_string = body_to_string(response_body).await;
        let items = response.items;
        if items.is_none() {
            error!("Error while fetching chat messages: Items is none! Response: {}", body_string);
            continue;
        }
        page_token = response.next_page_token;
        let wait_for_millis = response.polling_interval_millis.unwrap();
        let items = items.unwrap();
        for msg in items {
            let author_details = msg.author_details.unwrap();
            let channel_id = author_details.channel_id.unwrap();
            let display_name = author_details.display_name.unwrap();
            let message_snippet = msg.snippet.unwrap();
            let message_type = message_snippet.type_.unwrap();
            let published_at = message_snippet.published_at.unwrap();
            let sent_at = chrono::DateTime::parse_from_rfc3339(published_at.as_str()).unwrap();
            let received_at = chrono::Utc::now();
            let sent_at_timestamp = Timestamp { seconds: sent_at.timestamp() as i64, nanos: sent_at.timestamp_subsec_nanos() as i32 };
            let received_at_timestamp = Timestamp { seconds: received_at.timestamp() as i64, nanos: received_at.timestamp_subsec_nanos() as i32 };

            match message_type.as_str() {
                "textMessageEvent" => {
                    let message_text = message_snippet.display_message.unwrap();
                    info!("{} >> {}", display_name, message_text);
                    let chat_message = YouTubeChatMessage {
                        channel_id,
                        display_name,
                        message: message_text,
                        sent_at_timestamp: Some(sent_at_timestamp),
                        received_at_timestamp: Some(received_at_timestamp),
                    };
                    tx.send(chat_message)?;
                },
                _ => {}
            }
        }
        
        tokio::time::sleep(Duration::from_millis(wait_for_millis.into())).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_log(env::var_os("DEBUG").is_some());
    debug!("Debug mode activated!");

    let env_addr = env::var_os("YTS_GRPC_ADDRESS");
    let mut addr: SocketAddr = "0.0.0.0:50051".parse()?;
    if let Some(osstr_addr) = env_addr {
        let str_addr = osstr_addr.into_string().unwrap();
        addr = str_addr.parse()?;
    }
    let hub = authenticate_google().await?;
    let hub_arc = Arc::new(hub);
    let livechat_id: String;
    let mut livechat_id_opt: Option<String> = None;
    while livechat_id_opt.is_none() {
        livechat_id_opt = if env::var("YTS_LIVECHAT_ID").is_ok() {
            Some(env::var("YTS_LIVECHAT_ID").unwrap())
        } else {
            get_livechat_id(&hub_arc).await
        };
        if livechat_id_opt.is_none() {
            error!("Unable to determine livechat ID, retrying in 30 seconds");
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }
    livechat_id = livechat_id_opt.unwrap();

    info!("Livechat ID determined: {}", livechat_id);

    let (tx, rx) = tokio::sync::broadcast::channel(100);
    let service = YouTubeServiceImpl::new(tx.clone(), hub_arc.clone(), livechat_id.clone());

    let (service_future, fetch_future) = tokio::join!(
        Server::builder()
            .add_service(YouTubeServiceServer::new(service))
            .serve(addr),
        fetch_messages(&hub_arc, livechat_id, tx)
    );

    return Ok(());
}
