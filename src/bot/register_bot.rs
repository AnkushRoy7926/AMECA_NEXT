use crate::bot::AMECA;
use crate::models::channel::Channel;
use crate::{BoxResult, DynError};
use poise::command;
use poise::futures_util::{Stream, StreamExt};
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::futures;
use sqlx::PgPool;
use tracing::{debug, error, info};

type Context<'a> = poise::Context<'a, AMECA, DynError>;

async fn autocomplete_channel<'a>(
    ctx: Context<'_>,
    partial: &'a str,
) -> impl Stream<Item = String> + 'a {
    let guild_id = ctx.guild_id().expect("Cannot get guild ID");

    let channel_name = sqlx::query_as::<_, Channel>("SELECT * from channel WHERE guild_id = $1")
        .bind(guild_id.get() as i64)
        .fetch_all(&ctx.data().db)
        .await
        .expect("Error getting autocomplete channels")
        .iter()
        .map(|channel| channel.channel_name.clone())
        .collect::<Vec<_>>();

    let channel_binding = channel_name.clone();
    futures::stream::iter(channel_binding.to_owned())
        .filter(move |name| futures::future::ready(name.starts_with(partial)))
        .map(|name| name.clone().to_string())
}

#[poise::command(
    prefix_command,
    slash_command,
    guild_only = true,
    subcommands("add", "remove"),
    subcommand_required,
    category = "administration"
)]
// Omit 'ctx' parameter here. It is not needed, because this function will never be called.
pub async fn log_channel(_: Context<'_>) -> BoxResult<()> {
    // This will never be called, because `subcommand_required` parameter is set
    Ok(())
}

#[command(
    slash_command,
    guild_only = "true",
    required_permissions = "MANAGE_CHANNELS",
    category = "administration"
)]
pub async fn remove(ctx: Context<'_>) -> BoxResult<()> {
    let guild_id = ctx.guild_id().expect("Cannot get guild ID");
    let _x = sqlx::query!(
        "UPDATE channel SET logging_channel = FALSE where logging_channel = TRUE and guild_id = $1",
        guild_id.get() as i64
    )
    .execute(&ctx.data().db)
    .await;

    match _x {
        Ok(x) => {
            if x.rows_affected() == 0 {
                ctx.say("No logging channel to deregister for this guild!")
                    .await?;
            } else {
                ctx.say("Logging channel was successfully removed.").await?;
            }
        }
        Err(e) => {
            ctx.say(format!("Failed to delete logging channel: {}", e))
                .await?;
            let guild_id = guild_id.get();
            error!(guild_id, "Failed to delete logging channel: {}", e);
        }
    }
    Ok(())
}
pub async fn check_existing_log_channel(
    guild_id: i64,
    pool: &PgPool,
) -> BoxResult<Option<Channel>> {
    let x = sqlx::query_as::<_, Channel>(
        "SELECT * FROM channel WHERE guild_id = $1 AND logging_channel = true",
    )
    .bind(guild_id)
    .fetch_optional(pool)
    .await?;

    Ok(x)
}

#[command(
    slash_command,
    guild_only = "true",
    required_permissions = "MANAGE_CHANNELS",
    category = "administration"
)]
pub async fn add(
    ctx: Context<'_>,
    #[description = "Select logging channel"]
    #[autocomplete = "autocomplete_channel"]
    channel: serenity::Channel,
) -> BoxResult<()> {
    let channel_id = channel.id().get() as i64;
    let guild_id = ctx.guild_id().expect("Cannot get guild ID").get() as i64;
    ctx.defer().await?;
    match check_existing_log_channel(guild_id, &ctx.data().db).await {
        Ok(Some(channel)) => {
            ctx.say("Logging channel already registered").await?;
            ctx.say(format!(
                "Deregister existing channel {} <{}>",
                channel.channel_id, channel.channel_name
            ))
            .await?;
            return Ok(());
        }
        Ok(None) => (),
        Err(e) => {
            error!(
                guild_id,
                channel_id, "Error checking existing logging channel: {}", e
            );
            ctx.say("Error checking existing logging channel").await?;
            return Err(e);
        }
    }
    info!(
        "Setting up logging channel: {} for guild {}",
        channel,
        ctx.guild_id().expect("Cannot get guild ID")
    );
    let x = sqlx::query!(
        "UPDATE channel SET logging_channel = $1 WHERE guild_id = $2 AND channel_id = $3",
        true,
        guild_id,
        channel_id
    )
    .execute(&ctx.data().db)
    .await;
    match x {
        Ok(affected_rows) => {
            debug!("Insertion affected {} rows", affected_rows.rows_affected());
            ctx.say("Set logging channel successfully").await?;
        }
        Err(e) => {
            ctx.say("Unable to set logging channel!").await?;
            error!("{e:#?}")
        }
    }
    Ok(())
}
