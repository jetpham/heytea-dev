use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "heytea", about = "Query the heytea.dev singleton API")]
struct Args {
    #[arg(long, env = "HEYTEA_API_URL", default_value = "https://api.heytea.dev")]
    api_url: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Status,
    WaitTime,
    Notice,
    ClosingNotice,
    History {
        #[arg(long, default_value = "24h")]
        range: String,
        #[arg(long)]
        bucket: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let client = heytea::Client::new(&args.api_url)?;

    let value = match args.command {
        Command::Status => serde_json::to_value(client.status().await?)?,
        Command::WaitTime => serde_json::to_value(client.wait_time().await?)?,
        Command::Notice => serde_json::to_value(client.notice().await?)?,
        Command::ClosingNotice => serde_json::to_value(client.closing_notice().await?)?,
        Command::History { range, bucket } => {
            serde_json::to_value(client.history(&range, bucket.as_deref()).await?)?
        }
    };

    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
