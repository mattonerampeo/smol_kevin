//! Requires the "client", "standard_framework", and "voice" features be enabled
//! in your Cargo.toml, like so:
//!
//! ```toml
//! [dependencies.serenity]
//! git = "https://github.com/serenity-rs/serenity.git"
//! features = ["client", "standard_framework", "voice"]
//! ```

use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};

use hound;
use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    framework::{
        standard::{
            CommandResult,
            macros::{command, group, hook},
        },
        StandardFramework,
    },
    model::{
        channel::Message,
        gateway::Ready,
        misc::Mentionable
    },
    Result as SerenityResult,
};
use serenity::model::prelude::GuildId;
use serenity::prelude::TypeMapKey;
use songbird::{
    CoreEvent,
    driver::{Config as DriverConfig, DecodeMode},
    Event,
    EventContext,
    EventHandler as VoiceEventHandler,
    SerenityInit,
    Songbird,
};
use tokio::sync::RwLock;

const BUFFER_SIZE: usize = 2880000;
const SPEC: hound::WavSpec = hound::WavSpec {
    channels: 2,
    sample_rate: 48000,
    bits_per_sample: 16,
    sample_format: hound::SampleFormat::Int,
};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

struct Buffer {
    buf: Vec<i16>,
    pos: usize,
}

impl Buffer {
    fn new() -> Self {
        Self{
            buf: vec![0;BUFFER_SIZE],
            pos: BUFFER_SIZE
        }
    }

    fn push(&mut self, val: &Vec<i16>) {
        for bits in val {
            self.pos = if self.pos < BUFFER_SIZE - 1 {self.pos + 1} else {0};
            self.buf[self.pos] = *bits;
        }
    }

    fn pop(&self) -> Vec<i16> {
        let start = if self.pos < BUFFER_SIZE - 1 {self.pos + 1} else {0};
        [&self.buf[start ..], &self.buf[.. start]].concat()
    }
}

struct Receiver {
    buffer: Arc<Mutex<HashMap<u32, Buffer>>>,
}

impl Receiver {
    pub fn new(buffer: Arc<Mutex<HashMap<u32, Buffer>>>) -> Self {
        // You can manage state here, such as a buffer of audio packet bytes so
        // you can later store them in intervals.
        Self { buffer }
    }
}

struct AudioBuffers; // void struct used to generate a typemap that holds all active lobbies

impl TypeMapKey for AudioBuffers {
    type Value = Arc<RwLock<HashMap<GuildId, Arc<Mutex<HashMap<u32, Buffer>>>>>>; // a game is held within a lobby. the text channel id is the lobby's unique code
}

#[async_trait]
impl VoiceEventHandler for Receiver {
    #[allow(unused_variables)]
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        use EventContext as Ctx;
        match ctx {
            Ctx::VoicePacket {audio, packet, payload_offset, payload_end_pad} => {
                // An event which fires for every received audio packet,
                // containing the decoded data.
                if let Some(audio) = audio {
                    let mut buffer = self.buffer.lock().unwrap();
                    if let Some(buffer) = buffer.get_mut(&packet.ssrc) {
                        buffer.push(audio);
                    } else {
                        let mut new_buffer = Buffer::new();
                        new_buffer.push(audio);
                        buffer.insert(packet.ssrc, new_buffer);
                    }
                }
            },
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
        .after(after);

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
        data.insert::<AudioBuffers>(Arc::new(RwLock::new(HashMap::default())));
    }

    let _ = client.start().await.map_err(|why| println!("Client ended: {:?}", why));
}

#[command]
#[aliases("j")]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    let channel_id = if let Some(id) = msg
        .guild(&ctx.cache)
        .await
        .unwrap()
        .voice_states
        .get(&msg.author.id)
        .and_then(|vs| vs.channel_id)
    {
        id
    } else {
        msg.reply(&ctx, "not in a voice channel").await?;
        return Ok(());
    };

    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialisation.").clone();

    let (handler_lock, conn_result) = manager.join(guild_id, channel_id).await;

    if let Ok(_) = conn_result {
        // NOTE: this skips listening for the actual connection result.
        let mut handler = handler_lock.lock().await;

        let audio_buffer: HashMap<u32, Buffer> = HashMap::new();
        let audio_buffer = Arc::new(Mutex::new(audio_buffer));
        {
            let data_write = ctx.data.write().await;
            let buffers_lock = data_write.get::<AudioBuffers>().expect("Typemap incomplete").clone();
            buffers_lock.write().await.insert(guild_id, audio_buffer.clone());
        }

        handler.add_global_event(
            CoreEvent::VoicePacket.into(),
            Receiver::new(audio_buffer.clone()),
        );

        check_msg(msg.channel_id.say(&ctx.http, &format!("Joined {}", channel_id.mention())).await);
    } else {
        check_msg(msg.channel_id.say(&ctx.http, "Error joining the channel").await);
    }

    Ok(())
}

#[command]
#[aliases("l")]
#[only_in(guilds)]
async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialisation.").clone();
    let handler = manager.get(guild_id);

    if let Some(_) = handler {
        if let Err(e) = manager.remove(guild_id).await {
            check_msg(msg.channel_id.say(&ctx.http, format!("Failed: {:?}", e)).await);
        }

        check_msg(msg.channel_id.say(&ctx.http,"Left voice channel").await);
    } else {
        check_msg(msg.reply(ctx, "Not in a voice channel").await);
    }

    Ok(())
}

#[command]
#[aliases("d")]
#[only_in(guilds)]
async fn dump(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.expect("could not find guild from message");
    let guild_id = guild.id;
    let directory = &format!(".temp_audio/{}", guild_id)[..];
    if let Err(why) = std::fs::create_dir_all(directory) {
        eprintln!("error: {}", why)
    } else {
        let mut paths = Vec::new();
        {
            let data_read = ctx.data.read().await;
            let buffers_lock = data_read.get::<AudioBuffers>().expect("Typemap incomplete").clone();
            let buffer_lock = buffers_lock.read().await.get(&guild_id).expect("could not acquire a read lock on the data").clone();
            let mut buffer = buffer_lock.lock().expect("failed to get a write lock for buffer");
            for (id, buffer) in buffer.drain() {
                let path = format!("{}/{}.wav",directory, id);
                let mut writer = hound::WavWriter::create(&path, SPEC).unwrap();
                for sample in buffer.pop().iter() {
                    writer.write_sample(*sample)?;
                }
                writer.finalize().unwrap();
                paths.push(path);
            }
        }
        check_msg(msg.channel_id.send_message(ctx, |m| m.content("Starting the big DUMP")).await);
        for path in paths.iter() {
            msg.channel_id.send_message(ctx, |m| m.add_file(&path[..])).await.expect("Error sending audio files to discord");
            std::fs::remove_file(&path[..]).expect("failed to remove file");
        }
        check_msg(msg.channel_id.send_message(ctx, |m| m.content("done")).await);
    }
    Ok(())
}

#[command]
#[aliases("c")]
#[only_in(guilds)]
async fn clean(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.expect("could not find guild from message");
    let guild_id = guild.id;
    {
        let data_read = ctx.data.read().await;
        let buffers_lock = data_read.get::<AudioBuffers>().expect("Typemap incomplete").clone();
        let buffer_lock = buffers_lock.read().await.get(&guild_id).expect("could not acquire a read lock on the data").clone();
        let mut buffer = buffer_lock.lock().expect("failed to get a write lock for buffer");
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
