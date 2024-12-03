use std::str::FromStr;

use anyhow::{bail, Result};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

#[tokio::main]
async fn main() -> Result<()> {
    cmd_server().await?;
    Ok(())
}

pub async fn cmd_server() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:1234").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        println!("Accepted tcp connection");
        tokio::spawn(listen_to_cmds(socket));
    }
}

async fn listen_to_cmds(tcp: TcpStream) -> Result<()> {
    // let mut writer = LineWriter::new(tcp.try_clone()?);
    let (reader, mut writer) = tcp.into_split();
    let reader = BufReader::new(reader);

    let mut lines = reader.lines();
    while let Ok(Some(incoming_line)) = lines.next_line().await {
        println!("[Debug] raw cmd: {incoming_line}");
        let cmd = Cmd::from_str(incoming_line.trim_start())?;
        println!("[Debug] Cmd: {cmd:?}");

        // todo: grab for real
        let resp = match cmd {
            Cmd::Ping => Resp::Pong,
            Cmd::Msg(msg) => Resp::Msg(format!("received {msg}")),
            Cmd::SendToScript { .. } => Resp::Pong,
        };
        println!("[Debug] resp: {}", resp.to_string());

        writer.write_all(resp.to_string().as_bytes()).await?;
        writer.write_u8(b'\n').await?;
        writer.flush().await?;
    }
    println!("Client disconnected");

    Ok(())
}

#[derive(Debug)]
pub enum Cmd {
    Ping,
    Msg(String),
    SendToScript { script_label: String, msg: String },
}

pub enum Resp {
    Pong,
    Msg(String),
}

impl ToString for Resp {
    fn to_string(&self) -> String {
        match self {
            Resp::Pong => "pong".to_owned(),
            Resp::Msg(msg) => format!("msg {msg}"),
        }
    }
}

impl FromStr for Cmd {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Some(_) = s.strip_prefix("ping") {
            return Ok(Cmd::Ping);
        }
        if let Some(msg) = s.strip_prefix("msg") {
            let msg = msg.trim().to_owned();
            return Ok(Cmd::Msg(msg));
        }
        if let Some(rest) = s.strip_prefix("send") {
            let rest = rest.trim_start();
            // rest.split_once(o)
            if let Some((label, msg)) = rest.split_once(' ') {
                let msg = msg.trim().to_owned();
                return Ok(Cmd::SendToScript {
                    script_label: label.trim().to_owned(),
                    msg: msg.trim().to_owned(),
                });
            }
        }

        bail!("line does parse into a known cmd: {s}");
    }
}
