use justftp::Client;

fn get(port: u16, remote: &str, local: &str) -> Result<(), justftp::Error> {
    let mut ftp = Client::connect(("127.0.0.1", port))?;
    ftp.login("user", "pass")?;
    ftp.download(remote, local)?;
    ftp.quit()
}

fn main() -> Result<(), justftp::Error> {
    let port = justftp::fake::spawn();
    // --- call site ---
    std::thread::scope(|s| {
        let a = s.spawn(|| get(port, "/a", "d1.bin"));
        let b = s.spawn(|| get(port, "/b", "d2.bin"));
        a.join().unwrap().and(b.join().unwrap())
    })
}
