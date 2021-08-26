use google_youtube3::YouTube;
use hyper::{Body, Response};
use log::{error, info};
use yup_oauth2::DeviceFlowAuthenticator;

/// Because hyper stores the body weirdly, we need to first convert it to bytes (which works asynchronously) and then decode those bytes to UTF-8.
/// Thanks hyper.
pub async fn body_to_string(mut response: Response<Body>) -> String {
    let body_bytes = hyper::body::to_bytes(response.body_mut())
        .await
        .expect("msg");
    return String::from_utf8(body_bytes.to_vec()).unwrap();
}

/// Creates an authenticator that works with the device flow and immediately requests a token for the youtube scope.
/// Please be on the lookout for a message in the log for authenticating.
pub async fn authenticate_google() -> Result<(YouTube, YouTube), Box<dyn std::error::Error>> {
    let bot_secret = yup_oauth2::read_application_secret("clientsecret.json")
        .await
        .expect("clientsecret.json");
    let streamer_secret = yup_oauth2::read_application_secret("clientsecret.json")
        .await
        .expect("clientsecret.json");

    let bot_auth = DeviceFlowAuthenticator::builder(bot_secret)
        .persist_tokens_to_disk("tokencache_bot.json")
        .build()
        .await
        .unwrap();
    let streamer_auth = DeviceFlowAuthenticator::builder(streamer_secret)
        .persist_tokens_to_disk("tokencache_streamer.json")
        .build()
        .await
        .unwrap();
    info!("---------- BOT AUTHENTICATION ----------");
    let _ = bot_auth
        .token(&[
            "https://www.googleapis.com/auth/youtube",
            "https://www.googleapis.com/auth/youtube.readonly",
        ])
        .await;
    info!("---------- END BOT AUTHENTICATION ----------");
    info!("---------- STREAMER AUTHENTICATION ----------");
    let _ = streamer_auth
        .token(&[
            "https://www.googleapis.com/auth/youtube",
            "https://www.googleapis.com/auth/youtube.readonly",
        ])
        .await;
    info!("---------- END STREAMER AUTHENTICATION ----------");
    let bot_hub = YouTube::new(
        hyper::Client::builder().build(hyper_rustls::HttpsConnector::with_native_roots()),
        bot_auth,
    );
    let streamer_hub = YouTube::new(
        hyper::Client::builder().build(hyper_rustls::HttpsConnector::with_native_roots()),
        streamer_auth,
    );
    return Ok((bot_hub, streamer_hub));
}

/// Get the livechat id for the currently signed in user of the hub.
pub async fn get_livechat_id(hub: &YouTube) -> Option<String> {
    let broadcasts_response = hub
        .live_broadcasts()
        .list(&vec!["".to_string()])
        .broadcast_status("active")
        .broadcast_type("all")
        .doit()
        .await;
    if let Err(e) = broadcasts_response {
        error!("Unable to fetch livechat id: {}", e);
        return None;
    }
    let (_, response) = broadcasts_response.expect("msg");
    let items = response.items;

    match items {
        Some(broadcasts) => {
            if broadcasts.len() == 0 {
                return None;
            }
            let first_broadcast = broadcasts.get(0).unwrap();
            let snippet_option = &first_broadcast.snippet;
            let snippet = snippet_option.as_ref().unwrap();
            return snippet.live_chat_id.clone();
        }
        None => return None,
    }
}
