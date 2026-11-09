//! PROTOTYPE: blocking driver over the sans-IO core.
use crate::core::{parse_mlsd, Action, Core, Entry, Error};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};

pub struct Client {
    r: BufReader<TcpStream>,
    w: TcpStream,
    peer: SocketAddr,
    core: Core,
}

impl Client {
    pub fn connect(addr: impl ToSocketAddrs) -> Result<Self, Error> {
        let w = TcpStream::connect(addr)?;
        let peer = w.peer_addr()?;
        let mut c = Client { r: BufReader::new(w.try_clone()?), w, peer, core: Core::default() };
        c.core.greeting();
        c.drive(&mut io::sink(), &mut io::empty())?;
        Ok(c)
    }
    fn drive(&mut self, sink: &mut dyn Write, src: &mut dyn Read) -> Result<(), Error> {
        let mut data = None;
        loop {
            match self.core.next()? {
                Action::Send(l) => self.w.write_all(format!("{l}\r\n").as_bytes())?,
                Action::Recv => {
                    let mut line = String::new();
                    self.r.read_line(&mut line)?;
                    self.core.on_line(&line)?;
                }
                Action::OpenData(p) => data = Some(TcpStream::connect((self.peer.ip(), p))?),
                Action::ReadData => {
                    io::copy(data.as_mut().unwrap(), sink)?;
                    data = None;
                }
                Action::WriteData => {
                    io::copy(src, data.as_mut().unwrap())?;
                    data = None;
                }
                Action::Done => return Ok(()),
            }
        }
    }
    pub fn login(&mut self, user: &str, pass: &str) -> Result<(), Error> {
        self.core.login(user, pass);
        self.drive(&mut io::sink(), &mut io::empty())
    }
    pub fn list(&mut self, path: &str) -> Result<Vec<Entry>, Error> {
        self.core.list(path);
        let mut raw = Vec::new();
        self.drive(&mut raw, &mut io::empty())?;
        Ok(parse_mlsd(&raw))
    }
    pub fn download(&mut self, remote: &str, local: &str) -> Result<(), Error> {
        self.core.retr(remote);
        self.drive(&mut std::fs::File::create(local)?, &mut io::empty())
    }
    pub fn upload(&mut self, local: &str, remote: &str) -> Result<(), Error> {
        self.core.stor(remote);
        self.drive(&mut io::sink(), &mut std::fs::File::open(local)?)
    }
    pub fn quit(mut self) -> Result<(), Error> {
        self.core.quit();
        self.drive(&mut io::sink(), &mut io::empty())
    }
}
