use tokio::io::AsyncWriteExt;

pub async fn log(message: &str) {
    let mut stdout = tokio::io::stdout();
    stdout.write_all(message.as_bytes()).await.unwrap();
    stdout.flush().await.unwrap();
}
