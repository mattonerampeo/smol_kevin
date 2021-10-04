use serenity::Client;
use songbird::{
    Config,
    CoreEvent,
    driver::DecodeMode,
    Event,
    EventContext,
    EventHandler as VoiceEventHandler,
    model::payload::{ClientConnect, ClientDisconnect, Speaking},
    SerenityInit,
};
use handler::Handler;

mod setup_utils;
mod handler;

#[tokio::main]
async fn main() {
    let config = setup_utils::load_bot_config();

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