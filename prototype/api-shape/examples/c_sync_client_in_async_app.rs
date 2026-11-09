// Case: an async app (web handler) when the crate offers only the sync client.
use justftp::Client;

async fn handler(port: u16) -> Result<Vec<String>, justftp::Error> {
    // --- call site ---
    tokio::task::spawn_blocking(move || {
        let mut ftp = Client::connect(("127.0.0.1", port))?;
        ftp.login("user", "pass")?;
        let names = ftp.list("/pub")?.into_iter().map(|e| e.name).collect();
        ftp.quit()?;
        Ok(names)
    })
    .await
    .expect("worker panicked")
}

#[tokio::main]
async fn main() -> Result<(), justftp::Error> {
    println!("{:?}", handler(justftp::fake::spawn()).await?);
    Ok(())
}
