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
use crate::commands::move_to;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        ctx.shard.set_activity(Some(Activity::listening("...YOU...")));
        let application_id = ready.application.id.0; // usually this will be the bot's UserId
        let update = discord_update();

        let new_interaction = |name: String, description: String| async {
            if update {
                let _ = Interaction::create_global_application_command(&ctx,  application_id, |a| {
                    a.name(name)
                        .description(description)
                }).await;
            }
        };

        new_interaction(String::from("dump"), String::from("Dumps the audio buffer for the current channel in chat.")).await;
        new_interaction(String::from("clear"), String::from("Clears the audio buffer.")).await;
        new_interaction(String::from("join"), String::from("Makes the bot join your voice channel.")).await;
        new_interaction(String::from("leave"), String::from("Makes the bot leave your voice channel.")).await;
        new_interaction(String::from("follow"), String::from("Makes the bot follow you around.")).await;
        new_interaction(String::from("unfollow"), String::from("Makes the bot stop following you.")).await;



        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("dump")
                .description("test")
        }).await;
        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("clear")
                .description("test")
        }).await;
        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("join")
                .description("test")
        }).await;
        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("leave")
                .description("test")
        }).await;
        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("follow")
                .description("test")
        }).await;
        let _ = Interaction::create_guild_application_command(&ctx, GuildId(737641790856888320), application_id, |a| {
            a.name("unfollow")
                .description("test")
        }).await;


        println!("{} is online!", ready.user.name);
    }
    async fn voice_state_update(&self, ctx: Context, guild_id: Option<GuildId>, old: Option<VoiceState>, new: VoiceState) {
        let data_read = ctx.data.read().await;
        let follow_flag = data_read.get::<FollowFlag>().expect("Typemap incomplete").clone();
        let user_id = new.user_id;
        let guild_id = guild_id.unwrap();
        let guild = ctx.cache.guild(guild_id).await.unwrap();
        if user_id == ctx.cache.current_user_id().await {
            let flags = data_read.get::<JoinFlag>().expect("Typemap incomplete").clone();
            let mut flags = flags.lock().await;
            if flags.remove(&guild_id) == false {
                if let Some(old_vs) = old {
                    drop(flags);
                    let _ = move_to(&ctx, guild, old_vs.channel_id.unwrap()).await;
                }
            }
        } else if let Some(followed) = follow_flag.lock().await.get(&guild_id) {
            if followed == &user_id {
                if let Some(channel_id) = new.channel_id {
                    let _ = move_to(&ctx, guild, channel_id).await;
                }
            }
        };
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
                    "follow" => commands::follow(&ctx, response).await,
                    "unfollow" => commands::unfollow(&ctx, response).await,
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
        data.insert::<FollowFlag>(Arc::new(Mutex::new(HashMap::default())));
        data.insert::<JoinFlag>(Arc::new(Mutex::new(HashSet::default())));
    }

    let _ = client.start().await.map_err(|why| println!("Client ended: {:?}", why));
}

fn discord_token() -> String {
    env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment")
}

fn discord_update() -> bool {
    match env::var("DISCORD_UPDATE") {
        Ok(_) => true,
        Err(_) => false
    }
}