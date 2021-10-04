use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    model::{
        channel::Message,
        channel::ChannelType,
        gateway::Ready,
        guild::Guild,
        id::ChannelId,
        id::GuildId,
        id::UserId,
        misc::Mentionable
    },
    Result as SerenityResult,

};
use serenity::model::interactions::application_command::ApplicationCommandInteraction;
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

struct Receiver;

impl Receiver {
    pub fn new() -> Self {
        // You can manage state here, such as a buffer of audio packet bytes so
        // you can later store them in intervals.
        Self { }
    }
}

#[async_trait]
impl VoiceEventHandler for Receiver {
    #[allow(unused_variables)]
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        use EventContext as Ctx;
        match ctx {
            Ctx::SpeakingStateUpdate(
                Speaking {speaking, ssrc, user_id, ..}
            ) => {
                // Discord voice calls use RTP, where every sender uses a randomly allocated
                // *Synchronisation Source* (SSRC) to allow receivers to tell which audio
                // stream a received packet belongs to. As this number is not derived from
                // the sender's user_id, only Discord Voice Gateway messages like this one
                // inform us about which random SSRC a user has been allocated. Future voice
                // packets will contain *only* the SSRC.
                //
                // You can implement logic here so that you can differentiate users'
                // SSRCs and map the SSRC to the User ID and maintain this state.
                // Using this map, you can map the `ssrc` in `voice_packet`
                // to the user ID and handle their audio packets separately.
                println!(
                    "Speaking state update: user {:?} has SSRC {:?}, using {:?}",
                    user_id,
                    ssrc,
                    speaking,
                );
            },
            Ctx::SpeakingUpdate(data) => {
                // You can implement logic here which reacts to a user starting
                // or stopping speaking.
                println!(
                    "Source {} has {} speaking.",
                    data.ssrc,
                    if data.speaking {"started"} else {"stopped"},
                );
            },
            Ctx::VoicePacket(data) => {
                // An event which fires for every received audio packet,
                // containing the decoded data.
                if let Some(audio) = data.audio {
                    println!("Audio packet's first 5 samples: {:?}", audio.get(..5.min(audio.len())));
                    println!(
                        "Audio packet sequence {:05} has {:04} bytes (decompressed from {}), SSRC {}",
                        data.packet.sequence.0,
                        audio.len() * std::mem::size_of::<i16>(),
                        data.packet.payload.len(),
                        data.packet.ssrc,
                    );
                } else {
                    println!("RTP packet, but no audio. Driver may not be configured to decode.");
                }
            },
            Ctx::RtcpPacket(data) => {
                // An event which fires for every received rtcp packet,
                // containing the call statistics and reporting information.
                println!("RTCP packet received: {:?}", data.packet);
            },
            Ctx::ClientConnect(
                ClientConnect {audio_ssrc, video_ssrc, user_id, ..}
            ) => {
                // You can implement your own logic here to handle a user who has joined the
                // voice channel e.g., allocate structures, map their SSRC to User ID.

                println!(
                    "Client connected: user {:?} has audio SSRC {:?}, video SSRC {:?}",
                    user_id,
                    audio_ssrc,
                    video_ssrc,
                );
            },
            Ctx::ClientDisconnect(
                ClientDisconnect {user_id, ..}
            ) => {
                // You can implement your own logic here to handle a user who has left the
                // voice channel e.g., finalise processing of statistics etc.
                // You will typically need to map the User ID to their SSRC; observed when
                // speaking or connecting.

                println!("Client disconnected: user {:?}", user_id);
            },
            _ => {
                // We won't be registering this struct for any more event classes.
                unimplemented!()
            }
        }

        None
    }
}

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

pub async fn join(ctx: &Context, guild_id: GuildId, connect_to: ChannelId) -> Result<(), ()> {
    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialisation.").clone();

    let (handler_lock, conn_result) = manager.join(guild_id, connect_to).await;

    if let Ok(_) = conn_result {
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

    } else {
        // handle a failed join
    }

    Ok(()) // once we handle fails I need to remove this and return a meaningful result
}

pub async fn leave(ctx: &Context, command: &ApplicationCommandInteraction) -> String {
    if let Some(guild_id) = command.guild_id {
        let manager = songbird::get(ctx).await
            .expect("Could not retrieve songbird manager");
        if let Some(call) = manager.get(guild_id){
            let _current_channel = call.lock().await.current_channel().clone();
            drop(call);
            if let Some(current_channel) = _current_channel {
                println!("done");
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