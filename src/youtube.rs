use google_youtube3::YouTube;
use hyper::{Body, Response};
use log::error;
use yup_oauth2::DeviceFlowAuthenticator;

pub async fn body_to_string(mut response: Response<Body>) -> String {
    let body_bytes = hyper::body::to_bytes(response.body_mut()).await.expect("msg");
    return String::from_utf8(body_bytes.to_vec()).unwrap();
}

pub async fn authenticate_google() -> Result<YouTube, Box<dyn std::error::Error>> {
    let secret = yup_oauth2::read_application_secret("clientsecret.json").await.expect("clientsecret.json");
    let auth = DeviceFlowAuthenticator::builder(secret)
        .persist_tokens_to_disk("tokencache.json")
        .build()
        .await
        .unwrap();
    let hub = YouTube::new(hyper::Client::builder().build(hyper_rustls::HttpsConnector::with_native_roots()), auth);
    let result = hub.channels()
        .list(&vec!["snippet".to_string()])
        .mine(true)
        .doit()
        .await;
    match result {
        Err(e) => {
            return Err(Box::new(e));
        },
        Ok(res) => println!("Success!"),
    }
    return Ok(hub);
}

pub async fn get_livechat_id(hub: &YouTube) -> Option<String> {
    let broadcasts_response = hub.live_broadcasts()
        .list(&vec!["".to_string()])
        .broadcast_status("active")
        .broadcast_type("all")
        .doit().await;
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
        },
        None => return None,
    }
}