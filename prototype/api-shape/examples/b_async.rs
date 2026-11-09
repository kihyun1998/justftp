use justftp::tokio::Client;

#[tokio::main]
async fn main() -> Result<(), justftp::Error> {
    let port = justftp::fake::spawn();
    // --- call site ---
    let mut ftp = Client::connect(("127.0.0.1", port)).await?;
    ftp.login("user", "pass").await?;
    for e in ftp.list("/pub").await? {
        println!("{}", e.name);
    }
    ftp.download("/pub/a.iso", "a2.iso").await?;
    ftp.upload("a2.iso", "/in/a.iso").await?;
    ftp.quit().await?;
    Ok(())
}
