use std::future::Future;

use anyhow::Result;
use deno_bevy_interop::agent_runtime::{ScriptManager, ToJs};
use tokio::select;

fn main() -> Result<()> {
    let manager = ScriptManager::new();
    let example_script = "./ts/example.ts";
    let s1 = "s1";
    let s2 = "s2";
    manager.run(s1.to_string(), example_script)?;
    manager.run(s2.to_string(), example_script)?;

    let s1_tx = manager.sender(s1);
    let s2_tx = manager.sender(s2);

    spawn_async(async move {
        loop {
            select! {
                msg = manager.recv(s1) => {
                    let Some(msg) = msg else {
                        continue;
                    };
                    println!("[{s1}]: {msg:?}");
                },
                msg = manager.recv(s2) => {
                    let Some(msg) = msg else {
                        continue;
                    };
                    println!("[{s2}]: {msg:?}");
                }
            }
        }
    });

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

fn spawn_async<F>(f: F)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    std::thread::spawn(move || {
        let _ = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move { tokio::spawn(f).await });
    });
}
