use serenity::model::prelude::application_command::ApplicationCommandInteraction;
use serenity::prelude::*;

use crate::handler::*;

pub async fn command_not_implemented(ctx: &Context, command: ApplicationCommandInteraction) {
    let _ = utils::check_serenity_result(utils::send_response(ctx, &command, "this command is not implemented, make to update the bot".to_string()).await);
}

pub async fn join(ctx: &Context, command: ApplicationCommandInteraction) {
    if let Ok(_) = utils::check_serenity_result(utils::send_deferred_response(ctx, &command).await) {
        let feedback = voice::join(ctx, &command).await;
        let _ = utils::check_serenity_result(utils::edit_deferred_response(ctx, &command, feedback).await);
    }
}

pub async fn leave(ctx: &Context, command: ApplicationCommandInteraction) {
    if let Ok(_) = utils::check_serenity_result(utils::send_deferred_response(ctx, &command).await) {
        let feedback = voice::leave(ctx, &command).await;
        let _ = utils::check_serenity_result(utils::edit_deferred_response(ctx, &command, feedback).await);
    }
}

pub async fn dump(ctx: &Context, command: ApplicationCommandInteraction) {
    if let Ok(_) = utils::check_serenity_result(utils::send_deferred_response(ctx, &command).await) {
        let feedback = "todo".to_string(); // todo
        let _ = utils::check_serenity_result(utils::edit_deferred_response(ctx, &command, feedback).await);
    }
}
