use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "heytea", about = "Query the heytea.dev API")]
struct Args {
    #[arg(long, env = "HEYTEA_API_URL", default_value = "https://api.heytea.dev")]
    api_url: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Locations,
    Location {
        #[arg(value_name = "SLUG")]
        slug: String,
    },
    Status {
        #[arg(value_name = "SLUG")]
        slug: String,
    },
    WaitTime {
        #[arg(value_name = "SLUG")]
        slug: String,
    },
    Notice {
        #[arg(value_name = "SLUG")]
        slug: String,
    },
    ClosingNotice {
        #[arg(value_name = "SLUG")]
        slug: String,
    },
    History {
        #[arg(value_name = "SLUG")]
        slug: String,
        #[arg(long, default_value = "24h")]
        range: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let client = heytea::Client::new(&args.api_url)?;

    let value = match args.command {
        Command::Locations => serde_json::to_value(client.locations().await?)?,
        Command::Location { slug } => serde_json::to_value(client.location(&slug).await?)?,
        Command::Status { slug } => serde_json::to_value(client.status_for_location(&slug).await?)?,
        Command::WaitTime { slug } => {
            serde_json::to_value(client.wait_time_for_location(&slug).await?)?
        }
        Command::Notice { slug } => serde_json::to_value(client.notice_for_location(&slug).await?)?,
        Command::ClosingNotice { slug } => {
            serde_json::to_value(client.closing_notice_for_location(&slug).await?)?
        }
        Command::History { slug, range } => {
            serde_json::to_value(client.history_for_location(&slug, &range).await?)?
        }
    };

    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
