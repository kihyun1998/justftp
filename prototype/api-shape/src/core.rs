//! PROTOTYPE: sans-IO protocol core. No sockets, no runtime.
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
}

#[derive(Debug)]
pub enum Error {
    Reply(u16, String),
    Protocol(String),
    Io(std::io::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub enum Action {
    Send(String),
    Recv,
    OpenData(u16),
    ReadData,
    WriteData,
    Done,
}

enum Step {
    Send(String),
    Expect(&'static [u16]),
    PassIfAsked(String),
    OpenEpsv,
    DataIn,
    DataOut,
}

#[derive(Default)]
pub struct Core {
    steps: VecDeque<Step>,
    awaiting: Option<&'static [u16]>,
    multi: Option<u16>,
    text: String,
    last: u16,
}

impl Core {
    fn set(&mut self, s: Vec<Step>) {
        self.steps = s.into();
    }
    pub fn greeting(&mut self) {
        self.set(vec![Step::Expect(&[220])]);
    }
    pub fn login(&mut self, user: &str, pass: &str) {
        self.set(vec![
            Step::Send(format!("USER {user}")),
            Step::Expect(&[230, 331]),
            Step::PassIfAsked(pass.into()),
        ]);
    }
    fn transfer(&mut self, cmd: String, data: Step) {
        self.set(vec![
            Step::Send("EPSV".into()),
            Step::Expect(&[229]),
            Step::OpenEpsv,
            Step::Send(cmd),
            Step::Expect(&[125, 150]),
            data,
            Step::Expect(&[226, 250]),
        ]);
    }
    pub fn list(&mut self, path: &str) {
        self.transfer(format!("MLSD {path}"), Step::DataIn);
    }
    pub fn retr(&mut self, path: &str) {
        self.transfer(format!("RETR {path}"), Step::DataIn);
    }
    pub fn stor(&mut self, path: &str) {
        self.transfer(format!("STOR {path}"), Step::DataOut);
    }
    pub fn quit(&mut self) {
        self.set(vec![Step::Send("QUIT".into()), Step::Expect(&[221])]);
    }

    pub fn next(&mut self) -> Result<Action, Error> {
        if self.awaiting.is_some() {
            return Ok(Action::Recv);
        }
        loop {
            match self.steps.pop_front() {
                None => return Ok(Action::Done),
                Some(Step::Send(s)) => return Ok(Action::Send(s)),
                Some(Step::Expect(c)) => {
                    self.awaiting = Some(c);
                    return Ok(Action::Recv);
                }
                Some(Step::PassIfAsked(p)) => {
                    if self.last == 331 {
                        self.steps.push_front(Step::Expect(&[230]));
                        return Ok(Action::Send(format!("PASS {p}")));
                    }
                }
                Some(Step::OpenEpsv) => {
                    let port = self
                        .text
                        .split('|')
                        .nth(3)
                        .and_then(|p| p.parse().ok())
                        .ok_or_else(|| Error::Protocol(self.text.clone()))?;
                    return Ok(Action::OpenData(port));
                }
                Some(Step::DataIn) => return Ok(Action::ReadData),
                Some(Step::DataOut) => return Ok(Action::WriteData),
            }
        }
    }

    /// Feeds one control-connection line; handles RFC 959 multi-line replies.
    pub fn on_line(&mut self, line: &str) -> Result<(), Error> {
        let line = line.trim_end();
        let code: Option<u16> = line.get(..3).and_then(|c| c.parse().ok());
        let sep = line.as_bytes().get(3).copied();
        match (self.multi, code, sep) {
            (None, Some(c), Some(b'-')) => {
                self.multi = Some(c);
                return Ok(());
            }
            (Some(m), Some(c), Some(b' ')) if m == c => self.multi = None,
            (Some(_), _, _) => return Ok(()),
            (None, Some(_), _) => {}
            (None, None, _) => return Err(Error::Protocol(line.into())),
        }
        let c = code.unwrap();
        self.last = c;
        self.text = line[3..].trim().to_string();
        let want = self.awaiting.take().ok_or_else(|| Error::Protocol(line.into()))?;
        if want.contains(&c) {
            Ok(())
        } else {
            self.steps.clear();
            Err(Error::Reply(c, self.text.clone()))
        }
    }
}

pub fn parse_mlsd(raw: &[u8]) -> Vec<Entry> {
    String::from_utf8_lossy(raw)
        .lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(_, n)| Entry { name: n.to_string() })
        .collect()
}
