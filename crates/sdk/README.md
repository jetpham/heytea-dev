# heytea

Rust SDK for the public `heytea.dev` location and wait-time API.

```rust,no_run
# async fn example() -> anyhow::Result<()> {
let client = heytea::Client::production()?;
let locations = client.locations().await?;
let status = client.status_for_location("downtown-metreon").await?;
println!("{:?}", status.pickup_wait_minutes);
# Ok(())
# }
```
