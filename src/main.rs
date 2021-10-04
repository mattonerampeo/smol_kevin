mod setup_utils;
mod handler;

use handler::Handler;
use serenity::Client;

#[tokio::main]
async fn main() {
    let config = setup_utils::load_bot_config();

    let mut client = Client::builder(config.bot_token)
        .event_handler(Handler)
        .application_id(config.app_id)
        .await
        .expect("Err creating client");

    let _ = client.start().await.map_err(|why| println!("Client ended: {:?}", why));
}