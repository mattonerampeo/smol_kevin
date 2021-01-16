use std::{collections::{HashMap, HashSet}, env, process::Stdio, sync::Arc};
use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    framework::{
        standard::{
            Args,
            CommandGroup,
            CommandResult,
            help_commands,
            HelpOptions,
            macros::{command, group, help, hook},
        },
        StandardFramework,
    },
    model::{
        id::ChannelId,
        channel::Message,
        gateway::{Activity, Ready},
        misc::Mentionable,
        prelude::{GuildId, UserId},
    },
    prelude::TypeMapKey,
    Result as SerenityResult,
};
use songbird::{
    CoreEvent,
    driver::{Config as DriverConfig, DecodeMode},
    Event,
    EventContext,
    EventHandler as VoiceEventHandler,
    model::payload::{ClientDisconnect, Speaking},
    SerenityInit,
    Songbird,
};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::{Mutex, RwLock},
    task,
};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        let prefix = bot_prefix();
        ctx.shard.set_activity(Some(Activity::playing(&format!("{}help", prefix)[..])));
        println!("{} is connected!", ready.user.name);
    }
}

struct Buffer {
    buf: Vec<i16>,
    pos: usize,
}

impl Buffer {
    fn new() -> Self {
        let size = buffer_size();
        Self {
            buf: vec![0; size],
            pos: 0,
        }
    }

    fn push(&mut self, val: &Vec<i16>) {
        let size = buffer_size();
        for bytes in val {
            self.buf[self.pos] = *bytes;
            self.pos = if self.pos < size - 1 { self.pos + 1 } else { 0 };
        }
    }

    fn pop(&self) -> Vec<i16> {
        let size = buffer_size();
        let start = if self.pos < size - 1 { self.pos } else { 0 };
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

#[hook]
pub async fn after(ctx: &Context, msg: &Message, _: &str, _: CommandResult) {
    msg.delete(&ctx.http).await.expect("failed to delete message");
}

#[group]
#[commands(join, leave, dump, clean)]
struct General;

#[tokio::main]
async fn main() {
    let token = discord_token();
    let prefix = bot_prefix();

    let framework = StandardFramework::new()
        .configure(|c| c
            .ignore_bots(true)
            .with_whitespace(true)
            .prefix(&prefix[..]))
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

            let _ = handler.deafen(true).await;
            let _ = handler.mute(true).await;

            handler.add_global_event(
                CoreEvent::VoicePacket.into(),
                Receiver::new(lobby.clone()),
            );

            handler.add_global_event(
                CoreEvent::SpeakingStateUpdate.into(),
                Receiver::new(lobby.clone()),
            );

            handler.add_global_event(
                CoreEvent::ClientDisconnect.into(),
                Receiver::new(lobby.clone()),
            );
            send_msg_embed_on_channel(ctx, msg.channel_id, &format!("Joined {}", channel_id.mention())[..]).await;
        } else {
            send_msg_embed_on_channel(ctx, msg.channel_id, "Error joining the channel").await;
        }

        Ok(())
    } else {
        send_msg_embed_on_channel(ctx, msg.channel_id, "User is not in a voice channel").await;
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
            eprintln!("Failed: {:?}", e);
        }
        send_msg_embed_on_channel(ctx, msg.channel_id, "Left voice channel").await;
    } else {
        send_msg_embed_on_channel(ctx, msg.channel_id, "Not in a voice channel").await;
    }

    // to prevent poison errors, whenever the bot leaves it deletes the buffer for the server
    {
        let data_write = ctx.data.write().await;
        let buffers_lock = data_write.get::<Lobbies>().expect("Typemap incomplete").clone();
        if let Some(_) = buffers_lock.write().await.remove(&guild_id) {
            send_msg_embed_on_channel(ctx, msg.channel_id, "Audio buffer has been deleted.").await;
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
    let data_read = ctx.data.read().await;
    let lobbies_lock = data_read.get::<Lobbies>().expect("Typemap incomplete").clone();
    if let Some(lobby_lock) = lobbies_lock.read().await.get(&guild_id).clone() {
        let dump_started_message = msg.channel_id.send_message(ctx, |m|
            m.content("starting to dump. Might take a while!")
        ).await;
        if let Ok(message_to_remove) = dump_started_message {
            let lobby = lobby_lock.0.lock().await;
            let ssrc_map = lobby_lock.1.lock().await;
            let encoded_buffers = Arc::new(Mutex::new(Vec::<(Vec<u8>, String)>::new()));
            let mut encoding_threads = Vec::new();
            let output_format = output_format();
            let typing = msg.channel_id.start_typing(ctx.as_ref())?;
            for (id, buffer) in lobby.iter() {
                if let Some(user_id) = ssrc_map.get(&id) {
                    if let Some(member) = &members.get(user_id) {
                        let buffer = buffer.pop();
                        let name = member.user.name.clone();
                        let encoded_buffers = encoded_buffers.clone();
                        let output_format = output_format.clone();
                        encoding_threads.push(
                            task::spawn(async move {
                            let mut child = Command::new("ffmpeg")
                                .args(
                                    &[
                                        "-f", "s16be", // format in input
                                        "-ac", "2", // audio channels in input
                                        "-ar", "48k", // audio rate
                                        "-i", "-", // input takes a pipe
                                        "-f", &output_format[..], // output format
                                        "-b:a", "96k", // output rate
                                        "-ac", "2", // output audio channels
                                        "-" // output takes a pipe
                                    ])
                                .stdin(Stdio::piped())
                                .stdout(Stdio::piped())
                                .stderr(Stdio::null())
                                .spawn().expect("could not spawn encoder");

                            let samples = get_bytes(&buffer);
                            let mut stdin = child.stdin.take().expect("failed to open stdin");
                            task::spawn(async move {
                                stdin.write_all(&samples[..]).await.unwrap();
                            });
                            let encoded = child.wait_with_output().await.unwrap().stdout;
                            encoded_buffers.lock().await.push((encoded, format!("{}.{}", name, output_format)));
                        }));
                    }
                }
            };

            for handle in encoding_threads.drain(..) {
                handle.await?;
            }
            typing.stop();
            send_files_embed_on_channel(ctx, msg.channel_id, "Here's what you asked for!", &*encoded_buffers.lock().await).await;
            message_to_remove.delete(ctx).await?;
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
        let buffer = &mut lobby_lock.0.lock().await;
        buffer.clear();
    }
    send_msg_embed_on_channel(ctx, msg.channel_id, "The buffer has been cleared. No need to thank me").await;
    Ok(())
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}

async fn send_msg_embed_on_channel (ctx: &Context, channel_id: ChannelId, message_content: &str) {
    check_msg(channel_id.send_message(ctx, |m| m.embed(|m| m.description(message_content))).await);
}

async fn send_files_embed_on_channel (ctx: &Context, channel_id: ChannelId, message_content: &str, files: &Vec<(Vec<u8>, String)>) {
    let files_with_references = files.iter()
        .map(|(audio, name)| (&audio[..], &name[..])).collect::<Vec<_>>();
    check_msg(channel_id.send_message(ctx, |m| m.add_files(files_with_references).embed(|m| m.description(message_content))).await);
}

fn get_bytes(origin: &Vec<i16>) -> Vec<u8> {
    let mut output = Vec::new();
    origin.iter().for_each(|&signal| signal.to_be_bytes().iter().for_each(|&byte| { output.push(byte) }));
    output
}

fn buffer_size () -> usize{
    match env::var("DISCORD_BUFFER_SIZE") {
        Ok(custom_size) => custom_size.parse::<usize>()
            .expect("make sure the custom buffer is valid!"),
        Err(_) => 1440000
    }
}

fn output_format() -> String {
    match env::var("DISCORD_OUTPUT_FORMAT") {
        Ok(custom_format) => custom_format,
        Err(_) => "ogg".to_string()
    }
}

fn discord_token() -> String {
    env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment")
}

fn bot_prefix() -> String {
    match env::var("DISCORD_PREFIX") {
        Ok(custom_prefix) => custom_prefix,
        Err(_) => "?".to_string()
    }
}