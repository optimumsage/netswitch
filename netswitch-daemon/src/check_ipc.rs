#[tokio::main]
async fn main() {
    let port = std::env::var("NETSWITCH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| {
            if std::env::var("NETSWITCH_DEV").is_ok() {
                51338
            } else {
                51337
            }
        });

    let url = format!("http://127.0.0.1:{}/status", port);
    println!("Checking IPC at {}...", url);

    let client = reqwest::Client::new();
    match client.get(url).send().await {
        Ok(res) => {
            println!("Status: {}", res.status());
            let body = res.text().await.unwrap();
            println!("Body: {}", body);
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
