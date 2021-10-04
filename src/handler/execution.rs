use serenity::model::channel::ChannelType;
use serenity::model::interactions::InteractionResponseType;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::prelude::*;
use crate::handler::voice;
use crate::handler::voice::find_user_voice_channel;

pub async fn command_not_implemented(ctx: &Context, command: ApplicationCommandInteraction) {
    if let Err(why) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message|
                    message
                        //.content("The command you requested is not implemented")
                        .create_embed(|embed| {
                            embed.description("The command you requested is not implemented")
                        }))
        })
        .await
    {
        println!("Cannot respond to slash command: {}", why);
    }
}

pub async fn join(ctx: &Context, command: ApplicationCommandInteraction) {
    if let Err(why) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message|
                    message
                        .create_embed(|embed| {
                            embed.description("The bot is joining your channel")
                        }))
        })
        .await
    {
        println!("Cannot respond to slash command: {}", why);
    } else {
        if let Some(guild_id) = command.guild_id {
            if let Ok(channel_id) = find_user_voice_channel(ctx, guild_id, command.user.id).await {
                if let Ok(_) = voice::join(ctx, guild_id, channel_id).await {
                    // todo: we were able to connect, give feedback
                } else {
                    // todo: we could not connect, give feedback
                }
            } else {
                // todo: user is not in a voice channel, give feedback
            }
        }
    }
}

pub async fn leave(ctx: &Context, command: ApplicationCommandInteraction) {
    if let Err(why) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::DeferredChannelMessageWithSource)
        })
        .await
    {
        println!("Cannot respond to slash command: {}", why);
    } else {
        let feedback = voice::leave(ctx, &command).await;
        command.edit_original_interaction_response(&ctx.http, |response| {
            response
                .create_embed(|embed| {
                    embed.description(feedback)
                })
        }).await;
    }
}

pub async fn dump(ctx: &Context, command: ApplicationCommandInteraction) {
    if let Err(why) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message|
                    message
                        .create_embed(|embed| {
                            embed.description("The bot is dumping your channel's audio")
                        }))
        })
        .await
    {
        println!("Cannot respond to slash command: {}", why);
    }
}
