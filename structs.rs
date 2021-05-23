use std::{
    collections::HashMap,
    sync::Arc,
    env
};
use tokio::{
    sync::{
        Mutex,
        RwLock
    },
};
use serenity::{
    Result as SerenityResult,
    model::{
        guild::Guild,
        prelude::{GuildId, UserId},
        interactions::{Interaction, InteractionResponseType, InteractionApplicationCommandCallbackDataFlags, ApplicationCommandInteractionData},
    },
    client::Context,
    prelude::TypeMapKey,
};
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub enum AudioState {
    Timestamp(Instant),
    Padding(Duration),
    Audio(i16),
    Null
}

pub struct Buffer {
    buf: Vec<AudioState>,
    pos: usize,
    silence_pos: Option<usize>,
    size: usize,
}

impl Buffer {
    pub fn new() -> Self {
        let size = buffer_size();
        Self {
            buf: vec![AudioState::Null; size],
            pos: 0,
            silence_pos: None,
            size,
        }
    }

    pub fn push_silence_end(&mut self) {
        if let Some(pos) = self.silence_pos {
            if let AudioState::Timestamp(time) = self.buf[pos] {
                    self.buf[pos] = AudioState::Padding(time.elapsed());
            }
        }
    }

    pub fn push_audio(&mut self, val: &Vec<i16>) {
        for bytes in val {
            self.buf[self.pos] = AudioState::Audio(*bytes);
            self.pos = if self.pos < self.size - 1 { self.pos + 1 } else { 0 };
        }
    }

    pub fn push_silence(&mut self) {
        self.buf[self.pos] = AudioState::Timestamp(Instant::now());
        self.silence_pos = Some(self.pos);
        self.pos = if self.pos < self.size - 1 { self.pos + 1 } else { 0 };
    }

    pub fn pop_compressed(&self) -> Vec<i16> {
        let start = if self.pos < self.size - 1 { self.pos } else { 0 };
        let to_unwrap: Vec<AudioState> = [&self.buf[start..], &self.buf[..start]].concat();
        to_unwrap.iter().filter_map(|elem| {
            match elem {
                AudioState::Audio(audio) => Some(*audio),
                _ => None
            }
        }).collect()
    }

    pub fn pop_uncompressed(&self) -> Vec<i16> {
        let start = if self.pos < self.size - 1 { self.pos } else { 0 };
        let mut audio_state_buffer: Vec<AudioState> = [&self.buf[start..], &self.buf[..start]].concat();
        audio_state_buffer.reverse();
        let now = Instant::now();
        let mut silence_duration: usize = 0;
        let mut output_chunks: Vec<Vec<i16>> = Vec::new();
        for elem in audio_state_buffer {
            match elem {
                AudioState::Audio(audio) => output_chunks.push(vec![audio]),
                AudioState::Padding(duration) => {
                    let padding = (duration.as_secs_f64() * 96000.0) as usize;
                    silence_duration += padding;
                    if silence_duration > (120 * 96000) {
                        break
                    } else {
                        output_chunks.push(vec![0; padding])
                    }
                },
                AudioState::Timestamp(time) => {
                    let duration = now.duration_since(time);
                    let padding = (duration.as_secs_f64() * 96000.0) as usize;
                    silence_duration += padding;
                    if silence_duration > (120 * 96000) {
                        break
                    } else {
                        output_chunks.push(vec![0; padding])
                    }
                },
                AudioState::Null => {},
            }
        }
        output_chunks.reverse();
        output_chunks.concat()
    }

}

pub struct Receiver {
    pub lobby: Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>,
}

impl Receiver {
    pub fn new(lobby: Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>) -> Self {
        // You can manage state here, such as a buffer of audio packet bytes so
        // you can later store them in intervals.
        Self { lobby }
    }
}

pub struct Lobbies; // void struct used to generate a typemap that holds all active lobbies

impl TypeMapKey for Lobbies {
    type Value = Arc<RwLock<HashMap<GuildId, Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>>>>; // a game is held within a lobby. the text channel id is the lobby's unique code
}

pub struct JoinFlag;

impl TypeMapKey for JoinFlag {
    type Value = Arc<Mutex<HashSet<GuildId>>>;
}

pub struct FollowFlag;

impl TypeMapKey for FollowFlag {
    type Value = Arc<Mutex<HashMap<GuildId, UserId>>>;
}

fn buffer_size () -> usize {
    match env::var("DISCORD_BUFFER_SIZE") {
        Ok(custom_size) => custom_size.parse::<usize>()
            .expect("make sure the custom buffer is valid!") / 2,
        Err(_) => 1440000 // it's 15 seconds of audio
    }
}

pub struct Response {
    interaction: Interaction,
}

impl Response {
    pub async fn new(ctx: &Context, interaction: Interaction) -> Result<Response, ()> {
        if let Ok(_) = interaction.create_interaction_response(ctx, |response| {
            response.interaction_response_data(|m| {
                m.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
            })
                .kind(InteractionResponseType::AcknowledgeWithSource) // WARN si comporta come DeferredChannelMessageWithSource
        }).await {
            Ok(Response{
                interaction,
            })
        } else {
            Err(())
        }
    }

    pub fn data(&self) -> &Option<ApplicationCommandInteractionData> {
        &self.interaction.data
    }

    pub async fn guild(&self, ctx: &Context) -> (Guild, GuildId) {
        let guild_id = self.interaction.guild_id;
        let guild = ctx.cache.guild(guild_id).await.unwrap();
        (guild, guild_id)
    }

    pub fn member(&self) -> UserId {
        self.interaction.member.user.id
    }

    pub async fn edit(&self, ctx: &Context, message_content: &str) {
        check(self.interaction.edit_original_interaction_response(ctx, application_id(ctx).await, |m| {
                m.content(message_content)
                    //.embed(|m| m.description(message_content))
        }).await)
    }

    pub async fn _delete(&self, ctx: &Context) {
        check(self.interaction.delete_original_interaction_response(ctx, application_id(ctx).await).await)
    }

    pub async fn follow_up(&self, ctx: &Context, message_content: &str) {
        check(self.interaction.create_followup_message(ctx, application_id(ctx).await, false, |m| {
            m.content(message_content)
                .embed(|m| m.description(message_content))
        }).await)
    }

    pub async fn follow_up_files(&self, ctx: &Context, files: &Vec<(Vec<u8>, String)>) {
        /*
            let files_with_references = files.iter()
            .map(|(audio, name)| (&audio[..], &name[..])).collect::<Vec<_>>();
            check_response(self.interaction.create_followup_message(ctx, application_id(ctx).await, false, |m| {
            m.content(message_content)
                .files(files_with_references)
                .embed(|m| m.description(message_content))
            }).await)
         */
        self.send_files_embed_on_channel(ctx, files).await
    }

    async fn send_files_embed_on_channel (&self, ctx: &Context, files: &Vec<(Vec<u8>, String)>) {
        let files_with_references = files.iter()
            .map(|(audio, name)| (&audio[..], &name[..])).collect::<Vec<_>>();
        check(self.interaction.channel_id.send_message(ctx, |m| m.add_files(files_with_references)).await);
    }
}

pub async fn application_id(ctx: &Context) -> u64 {
    ctx.cache.current_user_id().await.0
}

fn check<T>(result: SerenityResult<T>) {
    if let Err(why) = result {
        eprintln!("Error sending response: {:?}", why);
    }
}