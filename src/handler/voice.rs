
use serenity::{
    client::Context,
    model::{
        channel::ChannelType,
        id::ChannelId,
        id::GuildId,
        id::UserId,
    },
};
use serenity::model::interactions::application_command::ApplicationCommandInteraction;
use songbird::{
    CoreEvent,
};

use crate::handler::receiver::Receiver;

pub async fn find_user_voice_channel(ctx: &Context, guild_id: GuildId, user_id: UserId) -> Result<ChannelId, ()> {
    for (channel_id, channel) in guild_id.channels(ctx).await.unwrap() {
        if channel.kind == ChannelType::Voice {
            for member in channel.members(ctx).await.unwrap() {
                if member.user.id == user_id {
                    return Ok(channel_id)
                }
            }
        }
    }
    Err(())
}

pub async fn join(ctx: &Context, command: &ApplicationCommandInteraction) -> String {
    if let Some(guild_id) = command.guild_id {
        let manager = songbird::get(ctx).await
            .expect("Could not retrieve songbird manager");
                if let Ok(channel_id) = find_user_voice_channel(ctx, guild_id, command.user.id).await {
                    let (handler_lock, conn_result) = manager.join(guild_id, channel_id).await;
                        if let Err(_) = conn_result{
                            format!("~ ERROR: could not connect to channel ~")
                        } else {
                            // NOTE: this skips listening for the actual connection result.
                            let mut handler = handler_lock.lock().await;

                            handler.add_global_event(
                                CoreEvent::SpeakingStateUpdate.into(),
                                Receiver::new(),
                            );

                            handler.add_global_event(
                                CoreEvent::SpeakingUpdate.into(),
                                Receiver::new(),
                            );

                            handler.add_global_event(
                                CoreEvent::VoicePacket.into(),
                                Receiver::new(),
                            );

                            handler.add_global_event(
                                CoreEvent::RtcpPacket.into(),
                                Receiver::new(),
                            );

                            handler.add_global_event(
                                CoreEvent::ClientConnect.into(),
                                Receiver::new(),
                            );

                            handler.add_global_event(
                                CoreEvent::ClientDisconnect.into(),
                                Receiver::new(),
                            );

                            format!("Hi!")
                        }
                } else {
                    format!("You need to be in a voice channel to call the bot ~.-")
                }
    } else {
        format!("~ ERROR: this bot only works inside of a guild, no funny business with private channels ~")
    }
}

pub async fn leave(ctx: &Context, command: &ApplicationCommandInteraction) -> String {
    if let Some(guild_id) = command.guild_id {
        let manager = songbird::get(ctx).await
            .expect("Could not retrieve songbird manager");
        if let Some(call) = manager.get(guild_id){
            let _current_channel = call.lock().await.current_channel().clone();
            drop(call);
            if let Some(current_channel) = _current_channel {
                if let Ok(channel_id) = find_user_voice_channel(ctx, guild_id, command.user.id).await {
                    if current_channel.0 == channel_id.0 {
                        drop(current_channel);
                        if let Err(why) = manager.leave(guild_id).await{
                            format!("~ ERROR: could not disconnect from channel - why: {} ~", why)
                        } else {
                            "Bye!".into()
                        }
                    } else {
                        "You need to be in the bot's voice channel to disconnect it -.~".into()
                    }
                } else {
                    "You need to be in a voice channel to disconnect it ~.-".into()
                }
            } else {
                "~ ERROR: could not retrieve current voice channel ~".into()
            }
        } else {
            "The bot is not in a call, you are powerless in this situation.".into()
        }
    } else {
        "~ ERROR: this bot only works inside of a guild, no funny business with private channels ~".into()
    }
}