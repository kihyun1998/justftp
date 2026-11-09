//! PROTOTYPE: tokio driver over the same sans-IO core.
use crate::core::{parse_mlsd, Action, Core, Entry, Error};
use ::tokio::fs::File;
use ::tokio::io::{self, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use ::tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use ::tokio::net::{TcpStream, ToSocketAddrs};
use std::net::SocketAddr;

pub struct Client {
    r: BufReader<OwnedReadHalf>,
    w: OwnedWriteHalf,
    peer: SocketAddr,
    core: Core,
}

type Sink<'a> = &'a mut (dyn AsyncWrite + Unpin + Send);
type Src<'a> = &'a mut (dyn AsyncRead + Unpin + Send);

impl Client {
    pub async fn connect(addr: impl ToSocketAddrs) -> Result<Self, Error> {
        let s = TcpStream::connect(addr).await?;
        let peer = s.peer_addr()?;
        let (r, w) = s.into_split();
        let mut c = Client { r: BufReader::new(r), w, peer, core: Core::default() };
        c.core.greeting();
        c.drive(&mut io::sink(), &mut io::empty()).await?;
        Ok(c)
    }
    async fn drive(&mut self, sink: Sink<'_>, src: Src<'_>) -> Result<(), Error> {
        let mut data = None;
        loop {
            match self.core.next()? {
                Action::Send(l) => self.w.write_all(format!("{l}\r\n").as_bytes()).await?,
                Action::Recv => {
                    let mut line = String::new();
                    self.r.read_line(&mut line).await?;
                    self.core.on_line(&line)?;
                }
                Action::OpenData(p) => data = Some(TcpStream::connect((self.peer.ip(), p)).await?),
                Action::ReadData => {
                    let mut d: TcpStream = data.take().unwrap();
                    io::copy(&mut d, sink).await?;
                }
                Action::WriteData => {
                    let mut d: TcpStream = data.take().unwrap();
                    io::copy(src, &mut d).await?;
                    d.shutdown().await?;
                }
                Action::Done => return Ok(()),
            }
        }
    }
    pub async fn login(&mut self, user: &str, pass: &str) -> Result<(), Error> {
        self.core.login(user, pass);
        self.drive(&mut io::sink(), &mut io::empty()).await
    }
    pub async fn list(&mut self, path: &str) -> Result<Vec<Entry>, Error> {
        self.core.list(path);
        let mut raw = Vec::new();
        self.drive(&mut raw, &mut io::empty()).await?;
        Ok(parse_mlsd(&raw))
    }
    pub async fn download(&mut self, remote: &str, local: &str) -> Result<(), Error> {
        self.core.retr(remote);
        let mut f = File::create(local).await?;
        self.drive(&mut f, &mut io::empty()).await?;
        f.flush().await?;
        Ok(())
    }
    pub async fn upload(&mut self, local: &str, remote: &str) -> Result<(), Error> {
        self.core.stor(remote);
        let mut f = File::open(local).await?;
        self.drive(&mut io::sink(), &mut f).await
    }
    pub async fn quit(mut self) -> Result<(), Error> {
        self.core.quit();
        self.drive(&mut io::sink(), &mut io::empty()).await
    }
}
