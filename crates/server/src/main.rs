use std::time::Duration;

const TICK_INTERVAL: Duration = Duration::from_millis(50);

#[tokio::main]
async fn main() {
    let mut interval = tokio::time::interval(TICK_INTERVAL);
    loop {
        interval.tick().await;
        //todo
    }
}
