//! Requires the "client", "standard_framework", and "voice" features be enabled
//! in your Cargo.toml, like so:
//!
//! ```toml
//! [dependencies.serenity]
//! git = "https://github.com/serenity-rs/serenity.git"
//! features = ["client", "standard_framework", "voice"]
//! ```

use std::{
    collections::{HashMap, HashSet},
    env,
    sync::{Arc, Mutex}
};

use hound;

use serenity::{
    prelude::TypeMapKey,
    async_trait,
    client::{Client, Context, EventHandler},
    framework::{
        standard::{
            CommandResult,
            macros::{command, group, help, hook},
            Args,
            CommandGroup,
            help_commands,
            HelpOptions,
        },
        StandardFramework,
    },
    model::{
        channel::Message,
        gateway::{Ready, Activity},
        misc::Mentionable,
        prelude::{GuildId, UserId}
    },
    Result as SerenityResult,
};

use songbird::{
    CoreEvent,
    driver::{Config as DriverConfig, DecodeMode},
    Event,
    EventContext,
    EventHandler as VoiceEventHandler,
    model::payload::Speaking,
    SerenityInit,
    Songbird,
};
use tokio::sync::RwLock;

const BUFFER_SIZE: usize = 1440000;
const SPEC: hound::WavSpec = hound::WavSpec {
    channels: 2,
    sample_rate: 48000,
    bits_per_sample: 16,
    sample_format: hound::SampleFormat::Int,
};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        ctx.shard.set_activity(Some(Activity::playing("#help")));
        println!("{} is connected!", ready.user.name);
    }
}

struct Buffer {
    buf: Vec<i16>,
    pos: usize,
}

impl Buffer {
    fn new() -> Self {
        Self {
            buf: vec![0; BUFFER_SIZE],
            pos: BUFFER_SIZE,
        }
    }

    fn push(&mut self, val: &Vec<i16>) {
        for bits in val {
            self.pos = if self.pos < BUFFER_SIZE - 1 { self.pos + 1 } else { 0 };
            self.buf[self.pos] = *bits;
        }
    }

    fn pop(&self) -> Vec<i16> {
        let start = if self.pos < BUFFER_SIZE - 1 { self.pos + 1 } else { 0 };
        [&self.buf[start..], &self.buf[..start]].concat()
    }
}

struct Receiver {
    lobby: Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>,
}

impl Receiver {
    pub fn new(lobby: Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>) -> Self {
        // You can manage state here, such as a buffer of audio packet bytes so
        // you can later store them in intervals.
        Self { lobby }
    }
}

struct Lobbies; // void struct used to generate a typemap that holds all active lobbies

impl TypeMapKey for Lobbies {
    type Value = Arc<RwLock<HashMap<GuildId, Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>>>>; // a game is held within a lobby. the text channel id is the lobby's unique code
}

#[async_trait]
impl VoiceEventHandler for Receiver {
    #[allow(unused_variables)]
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        use EventContext as Ctx;
        match ctx {
            Ctx::VoicePacket { audio, packet, payload_offset, payload_end_pad } => {
                // An event which fires for every received audio packet,
                // containing the decoded data.
                if let Some(audio) = audio {
                    let buffer = &mut self.lobby.0.lock().unwrap();
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
                Speaking { speaking, ssrc, user_id, .. }
            ) => {
                // You can implement your own logic here to handle a user who has joined the
                // voice channel e.g., allocate structures, map their SSRC to User ID.
                let buffer = &mut self.lobby.1.lock().unwrap();
                if let Some(user_id) = user_id {
                    let id = user_id.0;
                    buffer.insert(*ssrc, UserId(id));
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

#[hook]
pub async fn after(ctx: &Context, msg: &Message, _: &str, _: CommandResult) {
    msg.delete(&ctx.http).await.expect("failed to delete message");
}

#[group]
#[commands(join, leave, dump, clean)]
struct General;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment");

    let framework = StandardFramework::new()
        .configure(|c| c
            .ignore_bots(true)
            .with_whitespace(true)
            .prefix("#"))
        .group(&GENERAL_GROUP)
        .after(after)
        .help(&MY_HELP);

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
        .framework(framework)
        .register_songbird_with(songbird.into())
        .await
        .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<Lobbies>(Arc::new(RwLock::new(HashMap::default())));
    }

    let _ = client.start().await.map_err(|why| println!("Client ended: {:?}", why));
}

#[help]
#[command_not_found_text = "Could not find: `{}`."]
#[no_help_available_text("FUCK OFF.")]
#[strikethrough_commands_tip_in_guild("")]
#[individual_command_tip =
"Hello!\n\
If you want more information about a specific command, just pass the command as argument."]
async fn my_help(
    context: &Context,
    msg: &Message,
    args: Args,
    help_options: &'static HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    let _ = help_commands::with_embeds(context, msg, args, help_options, groups, owners).await;
    Ok(())
}

#[command]
#[aliases("j")]
#[description = "Let the bot join your channel."]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    if let Some(channel_id) = msg
        .guild(&ctx.cache)
        .await
        .unwrap()
        .voice_states
        .get(&msg.author.id)
        .and_then(|vs| vs.channel_id)
    {
        let guild = msg.guild(&ctx.cache).await.unwrap();
        let guild_id = guild.id;

        let manager = songbird::get(ctx).await
            .expect("Songbird Voice client placed in at initialisation.").clone();

        let (handler_lock, conn_result) = manager.join(guild_id, channel_id).await;

        if let Ok(_) = conn_result {

            let audio_buffer: HashMap<u32, Buffer> = HashMap::new();
            let ssrc_map: HashMap<u32, UserId> = HashMap::new();
            let lobby = Arc::new((Mutex::new(audio_buffer), Mutex::new(ssrc_map)));
            {
                let data_write = ctx.data.write().await;
                let buffers_lock = data_write.get::<Lobbies>().expect("Typemap incomplete").clone();
                buffers_lock.write().await.insert(guild_id, lobby.clone());
            }

            // NOTE: this skips listening for the actual connection result.
            let mut handler = handler_lock.lock().await;

            handler.add_global_event(
                CoreEvent::VoicePacket.into(),
                Receiver::new(lobby.clone()),
            );

            handler.add_global_event(
                CoreEvent::SpeakingStateUpdate.into(),
                Receiver::new(lobby.clone()),
            );

            check_msg(msg.channel_id.say(&ctx.http, &format!("Joined {}", channel_id.mention())).await);
        } else {
            check_msg(msg.channel_id.say(&ctx.http, "Error joining the channel").await);
        }

        Ok(())
    } else {
        check_msg(msg.reply(&ctx, "not in a voice channel").await);
        Ok(())
    }
}

#[command]
#[aliases("l")]
#[description = "Let the bot leave the voice channel currently in use. (this clears the server audio buffers)"]
#[only_in(guilds)]
async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialisation.").clone();

    if let Some(_) = manager.get(guild_id) {
        if let Err(e) = manager.remove(guild_id).await {
            check_msg(msg.channel_id.say(&ctx.http, format!("Failed: {:?}", e)).await);
        }

        check_msg(msg.channel_id.say(&ctx.http, "Left voice channel").await);
    } else {
        check_msg(msg.reply(ctx, "Not in a voice channel").await);
    }

    // to prevent poison errors, whenever the bot leaves it deletes the buffer for the server
    {
        let data_write = ctx.data.write().await;
        let buffers_lock = data_write.get::<Lobbies>().expect("Typemap incomplete").clone();
        if let Some(_) = buffers_lock.write().await.remove(&guild_id) {
            check_msg(msg.channel_id.say(&ctx.http, "Audio buffer has been deleted.").await);
        };
    }

    Ok(())
}

#[command]
#[aliases("d")]
#[description = "Dump the guild's audio buffer in the text channel."]
#[only_in(guilds)]
async fn dump(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.expect("could not find guild from message");
    let members = guild.members;
    let guild_id = guild.id;
    let directory = &format!(".temp_audio/{}", guild_id)[..];
    if let Err(why) = std::fs::create_dir_all(directory) {
        eprintln!("error: {}", why)
    } else {
        let mut paths = Vec::new();
        {
            let data_read = ctx.data.read().await;
            let lobbies_lock = data_read.get::<Lobbies>().expect("Typemap incomplete").clone();
            if let Some(lobby_lock) = lobbies_lock.read().await.get(&guild_id).clone() {
                let mut lobby = lobby_lock.0.lock().unwrap();
                let ssrc_map = lobby_lock.1.lock().unwrap();
                for (id, buffer) in lobby.drain() {
                    if let Some(user_id) = ssrc_map.get(&id) {
                        if let Some(member) = &members.get(user_id) {
                            let name = member.user.name.clone();
                            let path = format!("{}/{}.wav", directory, name);
                            let mut writer = hound::WavWriter::create(&path, SPEC).unwrap();
                            for sample in buffer.pop().iter() {
                                writer.write_sample(*sample)?;
                            }
                            writer.finalize().unwrap();
                            paths.push(path);
                        }
                    }
                }
            };
        }
        for path in paths.iter() {
            msg.channel_id.send_message(ctx, |m| m.add_file(&path[..])).await.expect("Error sending audio files to discord");
            std::fs::remove_file(&path[..]).expect("failed to remove file");
        }
    }
    Ok(())
}

#[command]
#[aliases("c")]
#[description = "Clear the guild's audio buffer."]
#[only_in(guilds)]
async fn clean(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.expect("could not find guild from message");
    let guild_id = guild.id;
    {
        let data_read = ctx.data.read().await;
        let lobbies_lock = data_read.get::<Lobbies>().expect("Typemap incomplete").clone();
        let lobby_lock = lobbies_lock.read().await.get(&guild_id).expect("could not acquire a read lock on the data").clone();
        let buffer = &mut lobby_lock.0.lock().expect("could not acquire a read lock on the data");
        buffer.clear();
    }
    check_msg(msg.channel_id.send_message(ctx, |m| m.content("The buffer has been cleared. No need to thank me")).await);
    Ok(())
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}