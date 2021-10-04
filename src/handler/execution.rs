use serenity::model::interactions::InteractionResponseType;
use serenity::prelude::*;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;

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
    }
}

pub async fn leave(ctx: &Context, command: ApplicationCommandInteraction) {
    if let Err(why) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message|
                    message
                        .create_embed(|embed| {
                            embed.description("The bot is leaving your channel")
                        }))
        })
        .await
    {
        println!("Cannot respond to slash command: {}", why);
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
