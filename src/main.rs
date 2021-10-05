use serenity::Client;
use songbird::{
    Config,
    driver::DecodeMode,
    SerenityInit,
};

use handler::Handler;

mod setup;
mod handler;

#[tokio::main]
async fn main() {
    let config = setup::load_bot_config();

    let songbird_config = Config::default()
        .decode_mode(DecodeMode::Decode);

    let mut client = Client::builder(config.bot_token)
        .event_handler(Handler)
        .application_id(config.app_id)
        .register_songbird_from_config(songbird_config)
        .await
        .expect("Err creating client");

    let _ = client.start().await.map_err(|why| println!("Client ended: {:?}", why));
}