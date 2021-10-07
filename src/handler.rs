use async_trait::async_trait;
use serenity::model::prelude::*;
use serenity::model::prelude::application_command::*;
use serenity::prelude::*;

use execution::*;

use crate::setup::load_bot_config;

mod execution;
mod voice;
mod utils;
mod receiver;

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        let config = load_bot_config();
        // if the user wants to update the slash commands
        if config.set_up_commands {
            // override commands to update everything in a simple, direct way
            let _ = ApplicationCommand::set_global_application_commands(&ctx, |commands| {
                commands
                    .create_application_command(|command| {
                        command
                            .name("join")
                            .description("make the bot join a channel")
                    })
                    .create_application_command(|command| {
                        command
                            .name("leave")
                            .description("make the bot leave a channel")
                    })
                    .create_application_command(|command| {
                        command
                            .name("dump")
                            .description("make the bot dump a channel's audio recordings")
                    })
            }).await;
        }

        ctx.shard.set_activity(Some(Activity::listening("your soul")));
        println!("{} is online and connected!", ready.user.name);
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            match command.data.name.as_str() {
                "join" => join(&ctx, command).await,
                "leave" => leave(&ctx, command).await,
                "dump" => dump(&ctx, command).await,
                _ => command_not_implemented(&ctx, command).await,
            };
        }
    }
}
