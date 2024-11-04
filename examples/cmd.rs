use std::{
    io::{BufRead, BufReader, LineWriter, Write},
    net::TcpStream,
};

use anyhow::Result;

fn main() -> Result<()> {
    println!("Attempting to connect");
    let tcp = TcpStream::connect("127.0.0.1:1234")?;

    println!("Connected");

    let mut writer = LineWriter::new(tcp.try_clone()?);
    let mut reader = BufReader::new(tcp);

    let mut resp = String::with_capacity(1000);
    let stdin = std::io::stdin();
    for cmd in stdin.lines() {
        let cmd = cmd?;
        writer.write_all(cmd.as_bytes())?;
        writer.write_all(b"\n")?;

        reader.read_line(&mut resp)?;
        print!("> {resp}");
        resp.clear();
    }

    Ok(())
}
