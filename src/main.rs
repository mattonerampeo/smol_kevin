mod commands;
mod structs;

use std::{
    collections::HashMap,
    env,
    sync::Arc};
use serenity::{
    async_trait,
    client::{
        Client,
        Context,
        EventHandler
    },
    model::{
        gateway::{
            Activity,
            Ready
        },
        prelude::UserId,
        interactions::Interaction,
    },
};
use songbird::{
    driver::{
        Config as DriverConfig,
        DecodeMode
    },
    Event,
    EventContext,
    EventHandler as VoiceEventHandler,
    model::payload::{
        ClientDisconnect,
        Speaking
    },
    SerenityInit,
    Songbird,
};
use tokio::{
    sync::RwLock,
    sync::Mutex,
};
use dotenv;
use crate::structs::*;
use serenity::model::id::GuildId;
use serenity::model::prelude::VoiceState;
use std::collections::HashSet;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        ctx.shard.set_activity(Some(Activity::listening("...YOU...")));
        let application_id = ready.application.id.0; // usually this will be the bot's UserId
        let existing_commands = ctx.http.get_global_application_commands(application_id).await.unwrap();
        let existing_commands_set: HashSet<String> = existing_commands.iter().map(|command| command.name.clone()).collect();

        let new_interaction = |name: String, description: String| async {
            if existing_commands_set.contains(&name) == false {
                let _ = Interaction::create_global_application_command(&ctx,  application_id, |a| {
                    a.name(name)
                        .description(description)
                }).await;
            } else {
                eprintln!("Skipping {} as it's already registered", name)
            }
        };

        new_interaction(String::from("dump"), String::from("Dumps the audio buffer for the current channel in chat.")).await;
        new_interaction(String::from("clear"), String::from("Clears the audio buffer.")).await;
        new_interaction(String::from("join"), String::from("Makes the bot join your voice channel.")).await;
        new_interaction(String::from("leave"), String::from("Makes the bot leave your voice channel.")).await;

        /*
        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("dump")
                .description("Dump the audio buffer for the current channel in chat.")
        }).await;
        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("clear")
                .description("Clear the audio buffer.")
        }).await;
        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("join")
                .description("Make the bot join your voice channel.")
        }).await;
        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("leave")
                .description("Make the bot leave your voice channel.")
        }).await;
         */

        println!("{} is online!", ready.user.name);
    }
    async fn voice_state_update(&self, ctx: Context, guild_id: Option<GuildId>, old: Option<VoiceState>, new: VoiceState) {
        if new.user_id == ctx.cache.current_user_id().await {
            let data_read = ctx.data.read().await;
            let flags = data_read.get::<Flags>().expect("Typemap incomplete").clone();
            let mut flags = flags.lock().await;
            let guild_id = guild_id.unwrap();
            if flags.remove(&guild_id) == false {
                println!("reconnecting");
                if let Some(old_vs) = old {
                    flags.insert(guild_id);
                    let manager = songbird::get(&ctx).await
                        .expect("Could not get manager connection.").clone();
                    let _ = manager.join(guild_id, old_vs.channel_id.unwrap()).await;
                }
            }
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Ok(response) = Response::new(&ctx, interaction).await {
            match response.data() {
                None => {}
                Some(command) => match command.name.as_str() {
                    "dump"  => commands::dump(&ctx, response).await,
                    "clear" => commands::clear(&ctx, response).await,
                    "join"  => commands::join(&ctx, response).await,
                    "leave" => commands::leave(&ctx, response).await,
                    _ => {}
                }
            }
        }
    }

}

#[async_trait]
impl VoiceEventHandler for Receiver {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        use EventContext as Ctx;
        match ctx {
            Ctx::VoicePacket { audio, packet, .. } => {
                // An event which fires for every received audio packet,
                // containing the decoded data.
                if let Some(audio) = audio {
                    let buffer = &mut self.lobby.0.lock().await;
                    if let Some(buffer) = buffer.get_mut(&packet.ssrc) {
                        buffer.push(audio);
                    } else {
                        let mut new_buffer = Buffer::new();
                        new_buffer.push(audio);
                        buffer.insert(packet.ssrc, new_buffer);
                    }
                }
            }

            Ctx::SpeakingStateUpdate(
                Speaking { ssrc, user_id, .. }
            ) => {
                // You can implement your own logic here to handle a user who has joined the
                // voice channel e.g., allocate structures, map their SSRC to User ID.
                let ssrc_to_user_map = &mut self.lobby.1.lock().await;
                if let Some(user_id) = user_id {
                    let id = user_id.0;
                    ssrc_to_user_map.insert(*ssrc, UserId(id));
                }
            }

            Ctx::ClientDisconnect(ClientDisconnect { user_id, .. }) => {
                let ssrc_to_user_map = &mut self.lobby.1.lock().await;
                // loops the entire buffer in case the ssrc changed midway through
                for (mapped_ssrc, mapped_user_id) in ssrc_to_user_map.iter() {
                    if mapped_user_id.0 == user_id.0 {
                        let audio_buffer = &mut self.lobby.0.lock().await;
                        audio_buffer.remove(mapped_ssrc);
                    }
                }
            }

            _ => {
                // We won't be registering this struct for any more event classes.
                unimplemented!()
            }
        }

        None
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let token = discord_token();
    // Here, we need to configure Songbird to decode all incoming voice packets.
    // If you want, you can do this on a per-call basis---here, we need it to
    // read the audio data that other people are sending us!
    let songbird = Songbird::serenity();
    songbird.set_config(
        DriverConfig::default()
            .decode_mode(DecodeMode::Decode)
    );

    let mut client = Client::builder(&token)
        .event_handler(Handler)
        .register_songbird_with(songbird.into())
        .await
        .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<Lobbies>(Arc::new(RwLock::new(HashMap::default())));
        data.insert::<Flags>(Arc::new(Mutex::new(HashSet::new())));
    }

    let _ = client.start().await.map_err(|why| println!("Client ended: {:?}", why));
}

fn discord_token() -> String {
    env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment")
}