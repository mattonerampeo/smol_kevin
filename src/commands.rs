use std::{collections::{HashMap}, env, process::Stdio, sync::Arc};
use serenity::{
    client::Context,
    model::{
        id::ChannelId,
        channel::Message,
        misc::Mentionable,
        prelude::UserId,
        interactions::{Interaction, InteractionResponseType, InteractionApplicationCommandCallbackDataFlags},
    },
    Result as SerenityResult,
};
use songbird::{
    CoreEvent,
};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::Mutex,
    task,
};
use crate::structs::*;

pub async fn join(ctx: &Context, interaction: &Interaction) {
    let guild_id = interaction.guild_id;
    let guild = ctx.cache.guild(guild_id).await.unwrap();
    if let Some(channel_id) = guild
        .voice_states
        .get(&interaction.member.user.id)
        .and_then(|vs| vs.channel_id)
    {
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

            response_embed(ctx, interaction, &format!("Joined {}", channel_id.mention())[..]).await;
        } else {
            response_ephemeral(ctx, interaction, "Error joining the channel").await;
        }
    } else {
        response_ephemeral(ctx, interaction, "User is not in a voice channel").await;
    }
}

pub async fn leave(ctx: &Context, interaction: &Interaction) {
    let guild_id = interaction.guild_id;
    let guild = ctx.cache.guild(guild_id).await.unwrap();
    if let Some(current_state) = guild
        .voice_states
        .get(&ctx.cache.current_user_id().await)
    {
        if let Some(channel_id) = guild
            .voice_states
            .get(&interaction.member.user.id)
            .and_then(|vs| vs.channel_id)
            .filter(|user_channel_id| *user_channel_id == current_state.channel_id.unwrap())
        {
            let manager = songbird::get(ctx).await
                .expect("Songbird Voice client placed in at initialisation.").clone();
            if let Some(call) = manager.get(guild_id) {
                if let Ok(_) = call.lock().await.leave().await {
                    response_embed(ctx, interaction, &format!("Left {}", channel_id.mention())[..]).await;
                } else {
                    response_ephemeral(ctx, interaction, &format!("Error: Could not leave {}", channel_id.mention())[..]).await;
                }
            }
            // to prevent poison errors, whenever the bot leaves it deletes the buffer for the server
            {
                let data_write = ctx.data.write().await;
                let buffers_lock = data_write.get::<Lobbies>().expect("Typemap incomplete").clone();
                buffers_lock.write().await.remove(&guild_id);
            }
        } else {
            response_ephemeral(ctx, interaction, "You have to be in the same channel as the bot to remove it").await;
        }
    } else {
        response_ephemeral(ctx, interaction, "The bot is not in a voice channel").await;
    }
}

pub async fn dump(ctx: &Context, interaction: &Interaction) {
    let guild_id = interaction.guild_id;
    let guild = ctx.cache.guild(guild_id).await.unwrap();
    let members = guild.members;
    let data_read = ctx.data.read().await;
    let lobbies_lock = data_read.get::<Lobbies>().expect("Typemap incomplete").clone();
    if let Some(lobby_lock) = lobbies_lock.read().await.get(&guild_id).clone() {
            let lobby = lobby_lock.0.lock().await;
            let ssrc_map = lobby_lock.1.lock().await;
            let encoded_buffers = Arc::new(Mutex::new(Vec::<(Vec<u8>, String)>::new()));
            let mut encoding_threads = Vec::new();
            let output_format = output_format();
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
                handle.await.unwrap();
            }
            response_files(ctx, interaction, "Done!", &*encoded_buffers.lock().await).await;
    };
}

pub async fn clear(ctx: &Context, interaction: &Interaction) {
    let guild_id = interaction.guild_id;
    {
        let data_read = ctx.data.read().await;
        let lobbies_lock = data_read.get::<Lobbies>().expect("Typemap incomplete").clone();
        let lobby_lock = lobbies_lock.read().await.get(&guild_id).expect("could not acquire a read lock on the data").clone();
        let buffer = &mut lobby_lock.0.lock().await;
        buffer.clear();
    }
    response_embed(ctx, interaction, "The buffer has been cleared. No need to thank me").await;
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}

fn check_response<T>(result: SerenityResult<T>) {
    if let Err(why) = result {
        println!("Error sending response: {:?}", why);
    }
}

async fn send_files_embed_on_channel (ctx: &Context, channel_id: ChannelId, message_content: &str, files: &Vec<(Vec<u8>, String)>) {
    let files_with_references = files.iter()
        .map(|(audio, name)| (&audio[..], &name[..])).collect::<Vec<_>>();
    check_msg(channel_id.send_message(ctx, |m| m.add_files(files_with_references).embed(|m| m.description(message_content))).await);
}

async fn response_ephemeral(ctx: &Context, interaction: &Interaction, message_content: &str) {
    check_response(interaction.create_interaction_response(ctx, |response| {
        response.kind(InteractionResponseType::ChannelMessageWithSource);
        response.interaction_response_data(|m| {
            m.content(message_content)
            //.embed(|m| m.description(message_content))
                .flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
        })
    }).await)
}
async fn response_embed(ctx: &Context, interaction: &Interaction, message_content: &str) {
    check_response(interaction.create_interaction_response(ctx, |response| {
        response.kind(InteractionResponseType::ChannelMessageWithSource);
        response.interaction_response_data(|m| {
            m.content(message_content)
            //.embed(|m| m.description(message_content))
        })
    }).await)
}

async fn response_files(ctx: &Context, interaction: &Interaction, message_content: &str, files: &Vec<(Vec<u8>, String)>) {
    //let application_id = ctx.cache.current_user_id().await.0;
    send_files_embed_on_channel(ctx, interaction.channel_id, message_content, files).await;
    /*
    check_response(interaction.create_followup_message(ctx, application_id, false, |m| {
        m.add_files(files_with_references)
    }).await)
     */
}

fn get_bytes(origin: &Vec<i16>) -> Vec<u8> {
    let mut output = Vec::new();
    origin.iter().for_each(|&signal| signal.to_be_bytes().iter().for_each(|&byte| { output.push(byte) }));
    output
}

fn output_format() -> String {
    match env::var("DISCORD_OUTPUT_FORMAT") {
        Ok(custom_format) => custom_format,
        Err(_) => "ogg".to_string()
    }
}