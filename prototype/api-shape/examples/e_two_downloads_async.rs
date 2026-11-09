use justftp::tokio::Client;

async fn get(port: u16, remote: &str, local: &str) -> Result<(), justftp::Error> {
    let mut ftp = Client::connect(("127.0.0.1", port)).await?;
    ftp.login("user", "pass").await?;
    ftp.download(remote, local).await?;
    ftp.quit().await
}

#[tokio::main]
async fn main() -> Result<(), justftp::Error> {
    let port = justftp::fake::spawn();
    // --- call site ---
    let (a, b) = tokio::join!(get(port, "/a", "e1.bin"), get(port, "/b", "e2.bin"));
    a.and(b)
}
