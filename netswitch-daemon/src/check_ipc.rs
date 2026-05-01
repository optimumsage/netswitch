#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();
    match client.get("http://127.0.0.1:51337/status").send().await {
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
