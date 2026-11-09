use justftp::Client;

fn main() -> Result<(), justftp::Error> {
    let port = justftp::fake::spawn();
    // --- call site ---
    let mut ftp = Client::connect(("127.0.0.1", port))?;
    ftp.login("user", "pass")?;
    for e in ftp.list("/pub")? {
        println!("{}", e.name);
    }
    ftp.download("/pub/a.iso", "a.iso")?;
    ftp.upload("a.iso", "/in/a.iso")?;
    ftp.quit()?;
    Ok(())
}
