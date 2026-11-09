//! PROTOTYPE: scripted FTP server on a thread so the call sites really run.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

pub fn spawn() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || serve(s));
        }
    });
    port
}

fn serve(mut w: TcpStream) {
    let mut r = BufReader::new(w.try_clone().unwrap());
    let _ = w.write_all(b"220-welcome\r\n220 ready\r\n");
    let mut data: Option<TcpListener> = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let cmd = line.split_whitespace().next().unwrap_or("").to_string();
        eprintln!("    server got: {}", line.trim());
        let reply: &[u8] = match cmd.as_str() {
            "USER" => b"331 need password\r\n",
            "PASS" => b"230 ok\r\n",
            "QUIT" => b"221 bye\r\n",
            "EPSV" => {
                let d = TcpListener::bind("127.0.0.1:0").unwrap();
                let p = d.local_addr().unwrap().port();
                data = Some(d);
                let _ = w.write_all(format!("229 ok (|||{p}|)\r\n").as_bytes());
                continue;
            }
            "MLSD" | "RETR" => {
                let (mut d, _) = data.take().unwrap().accept().unwrap();
                let _ = w.write_all(b"150 go\r\n");
                let body: &[u8] = if cmd == "MLSD" {
                    b"type=file; a.iso\r\ntype=file; b.txt\r\n"
                } else {
                    b"hello from fake server\n"
                };
                let _ = d.write_all(body);
                drop(d);
                b"226 done\r\n"
            }
            "STOR" => {
                let (mut d, _) = data.take().unwrap().accept().unwrap();
                let _ = w.write_all(b"150 go\r\n");
                let mut got = Vec::new();
                let _ = d.read_to_end(&mut got);
                eprintln!("    server stored {} bytes", got.len());
                b"226 done\r\n"
            }
            _ => b"500 what\r\n",
        };
        let _ = w.write_all(reply);
        if cmd == "QUIT" {
            return;
        }
    }
}
