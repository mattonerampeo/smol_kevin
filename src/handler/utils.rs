use serenity::model::interactions::InteractionResponseType;
use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::model::prelude::Message;
use serenity::prelude::*;

pub async fn send_deferred_response(ctx: &Context, command: &ApplicationCommandInteraction) -> serenity::Result<()> {
    command.create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::DeferredChannelMessageWithSource)
        })
        .await
}

pub async fn send_response(ctx: &Context, command: &ApplicationCommandInteraction, text: String) -> serenity::Result<()> {
    command.create_interaction_response(&ctx.http, |response| {
        response
            .kind(InteractionResponseType::ChannelMessageWithSource)
            .interaction_response_data(|response| {
                response
                    .create_embed(|embed| {
                        embed.description(text)
                    })
            })
    })
        .await
}

pub async fn edit_deferred_response(ctx: &Context, command: &ApplicationCommandInteraction, text: String) -> serenity::Result<Message> {
        command.edit_original_interaction_response(&ctx.http, |response| {
            response
                .create_embed(|embed| {
                    embed.description(text)
                })
        }).await
}

pub fn check_serenity_result<T>(result: serenity::Result<T>) -> Result<(), ()> {
    if let Err(why) = result {
        eprintln!("Caught error from serenity: {}", why.to_string());
        Err(())
    } else {
        Ok(())
    }
}