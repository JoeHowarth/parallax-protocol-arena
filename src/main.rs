use agent_runtime::*;
use deno_core::error::AnyError;
use eyre::Result;

pub mod agent_runtime;

fn main() -> Result<(), AnyError> {
    let manager = ScriptManager::new();
    let example_script = "./ts/example.ts";
    let s1 = "s1";
    let s2 = "s2";
    let mut js_rx = manager.run(s1.to_string(), example_script)?;
    let mut js_rx_2 = manager.run(s2.to_string(), example_script)?;

    std::thread::spawn(move || {
        while let Some(msg) = js_rx.blocking_recv() {
            println!("Received msg: {:?}", msg);
        }
    });

    std::thread::spawn(move || {
        while let Some(msg) = js_rx_2.blocking_recv() {
            println!("2 Received msg: {:?}", msg);
        }
    });

    let s1_tx = manager.sender(s1);
    let s2_tx = manager.sender(s2);

    let stdin = std::io::stdin();
    for line in stdin.lines() {
        let line = line?;
        if line.starts_with("2") {
            s2_tx.blocking_send(ToJs::Msg(line))?;
        } else {
            s1_tx.blocking_send(ToJs::Msg(line))?;
        }
    }

    Ok(())
}
