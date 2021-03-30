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

pub struct Buffer {
    buf: Vec<i16>,
    pos: usize,
}

impl Buffer {
    pub fn new() -> Self {
        let size = buffer_size();
        Self {
            buf: vec![0; size],
            pos: 0,
        }
    }

    pub fn push(&mut self, val: &Vec<i16>) {
        let size = buffer_size();
        for bytes in val {
            self.buf[self.pos] = *bytes;
            self.pos = if self.pos < size - 1 { self.pos + 1 } else { 0 };
        }
    }

    pub fn pop(&self) -> Vec<i16> {
        let size = buffer_size();
        let start = if self.pos < size - 1 { self.pos } else { 0 };
        [&self.buf[start..], &self.buf[..start]].concat()
    }
}

pub struct Receiver {
    pub(crate) lobby: Arc<(Mutex<HashMap<u32, Buffer>>, Mutex<HashMap<u32, UserId>>)>,
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

fn buffer_size () -> usize {
    match env::var("DISCORD_BUFFER_SIZE") {
        Ok(custom_size) => custom_size.parse::<usize>()
            .expect("make sure the custom buffer is valid!"),
        Err(_) => 1440000
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

    pub async fn delete(&self, ctx: &Context) {
        check(self.interaction.delete_original_interaction_response(ctx, application_id(ctx).await).await)
    }

    pub async fn follow_up(&self, ctx: &Context, message_content: &str) {
        check(self.interaction.create_followup_message(ctx, application_id(ctx).await, false, |m| {
            m.content(message_content)
                .embed(|m| m.description(message_content))
        }).await)
    }

    pub async fn follow_up_files(&self, ctx: &Context, message_content: &str, files: &Vec<(Vec<u8>, String)>) {
        /*
            let files_with_references = files.iter()
            .map(|(audio, name)| (&audio[..], &name[..])).collect::<Vec<_>>();
            check_response(self.interaction.create_followup_message(ctx, application_id(ctx).await, false, |m| {
            m.content(message_content)
                .files(files_with_references)
                .embed(|m| m.description(message_content))
            }).await)
         */
        self.send_files_embed_on_channel(ctx, message_content, files).await
    }

    async fn send_files_embed_on_channel (&self, ctx: &Context, message_content: &str, files: &Vec<(Vec<u8>, String)>) {
        let files_with_references = files.iter()
            .map(|(audio, name)| (&audio[..], &name[..])).collect::<Vec<_>>();
        check(self.interaction.channel_id.send_message(ctx, |m| m.add_files(files_with_references).embed(|m| m.description(message_content))).await);
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
