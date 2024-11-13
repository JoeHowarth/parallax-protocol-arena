use std::{
    cell::RefCell,
    env,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use bevy::prelude::Resource;
use dashmap::DashMap;
use deno_ast::{MediaType, ParseParams, SourceMapOption};
use deno_core::{
    extension,
    op2,
    JsRuntime,
    ModuleCodeString,
    ModuleLoadResponse,
    ModuleName,
    ModuleSourceCode,
    OpState,
    SourceMapData,
};
use serde::{Deserialize, Serialize};
pub use tokio::sync::mpsc;
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = "bindings.ts")]
pub enum FromJs {
    Msg(String),
    Query(JsQuery),
    Action,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = "bindings.ts")]
pub enum ToJs {
    Msg(String),
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = "bindings.ts")]
pub struct JsQuery {
    key: String,
}

#[derive(Resource)]
pub struct ScriptManager {
    txs: Arc<DashMap<String, mpsc::Sender<ToJs>>>,
    rxs: Arc<DashMap<String, mpsc::Receiver<FromJs>>>,
    new_runtime: mpsc::UnboundedSender<(String, PathBuf, oneshot::Sender<()>)>,
}

impl Clone for ScriptManager {
    fn clone(&self) -> Self {
        Self {
            txs: Arc::clone(&self.txs),
            rxs: Arc::clone(&self.rxs),
            new_runtime: self.new_runtime.clone(),
        }
    }
}

// pub struct ScriptManagerHandle {}

pub type ScriptSender = mpsc::Sender<ToJs>;
// pub struct ScriptHandle(pub mpsc::Sender<ToJs>);

impl ScriptManager {
    pub fn try_send(&self, name: &str, val: ToJs) -> Option<()> {
        self.txs.get(name).unwrap().try_send(val).ok()
    }

    pub fn sender(&self, name: &str) -> mpsc::Sender<ToJs> {
        self.txs.get(name).unwrap().clone()
    }

    pub async fn recv(&self, name: &str) -> Option<FromJs> {
        self.rxs.get_mut(name)?.recv().await
    }

    pub fn try_recv(&self, name: &str) -> Option<FromJs> {
        self.rxs.get_mut(name)?.try_recv().ok()
    }

    pub fn run(&self, name: String, path: impl Into<PathBuf>) -> Result<()> {
        let (send_done, is_done) = oneshot::channel();
        self.new_runtime.send((name, path.into(), send_done))?;
        is_done.blocking_recv().map_err(Into::into)
    }

    pub fn new() -> ScriptManager {
        let (new_runtime, mut rx) = mpsc::unbounded_channel();
        let manager = ScriptManager {
            txs: Default::default(),
            rxs: Default::default(),
            new_runtime,
        };

        {
            let manager = manager.clone();
            JsRuntime::init_platform(None, false);
            std::thread::spawn(move || {
                while let Some((name, file, send_done)) = rx.blocking_recv() {
                    manager.run_inner(name, file, send_done)
                }
            });
        }

        manager
    }

    fn run_inner(
        &self,
        name: String,
        path: impl Into<PathBuf>,
        send_done: oneshot::Sender<()>,
    ) {
        let (js_tx, rust_rx) = mpsc::channel(10);
        let (rust_tx, js_rx) = mpsc::channel(10);
        self.txs.insert(name.clone(), js_tx);
        self.rxs.insert(name, js_rx);

        let path = path.into();
        std::thread::spawn(move || {
            let tokio_runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            if let Err(error) =
                tokio_runtime.block_on(run_js(path, rust_rx, rust_tx))
            {
                eprintln!("error: {}", error);
            }
        });

        let _ = send_done.send(());
    }
}

async fn run_js(
    file_path: PathBuf,
    rx: mpsc::Receiver<ToJs>,
    tx: mpsc::Sender<FromJs>,
) -> Result<()> {
    let main_module =
        deno_core::resolve_path(file_path, &std::env::current_dir()?)?;
    // let bindings_module =
    //     deno_core::resolve_path("./bindings/bindings.ts",
    // &std::env::current_dir()?)?;
    let mut js_runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
        module_loader: Some(Rc::new(TsModuleLoader)),
        extensions: vec![runjs::init_ops_and_esm(rx, tx)],
        extension_transpiler: Some(Rc::new(|specifier, source| {
            maybe_transpile_source(specifier, source)
        })),
        ..Default::default()
    });

    // let bindings_id =
    // js_runtime.load_side_es_module(&bindings_module).await?;
    let mod_id = js_runtime.load_main_es_module(&main_module).await?;
    let result = js_runtime.mod_evaluate(mod_id);
    js_runtime.run_event_loop(Default::default()).await?;
    result.await
}

extension! {
    runjs,
    ops = [
        op_read_file,
        op_write_file,
        op_remove_file,
        op_fetch,
        op_send,
        op_recv,
        op_sleep,
    ],
    // config = { mint: usize },
    esm_entry_point = "ext:runjs/runtime.ts",
    esm = [dir "ts", "runtime.ts"],
    options = {
        rx: mpsc::Receiver<ToJs>,
        tx: mpsc::Sender<FromJs>,
    },
    state = |state: &mut OpState, options: Config| {
        // Initialize state when extension loads
        state.put(RefCell::new(options.rx));
        state.put(options.tx);
    },
}

#[op2(async)]
async fn op_sleep(ms: u32) -> Result<()> {
    tokio::time::sleep(Duration::from_millis(ms as u64)).await;
    Ok(())
}

#[op2(async)]
async fn op_send(
    state: Rc<RefCell<OpState>>,
    #[serde] msg: FromJs,
) -> Result<()> {
    let state = state.borrow();
    let tx: &mpsc::Sender<FromJs> = state.borrow();

    tx.send(msg).await.map_err(Into::into)
}

#[op2(async)]
#[serde]
async fn op_recv(state: Rc<RefCell<OpState>>) -> Result<ToJs> {
    let state = state.borrow();
    let rx: &RefCell<mpsc::Receiver<ToJs>> = state.borrow();
    let mut rx = rx.borrow_mut();

    Ok(rx
        .recv()
        .await
        .unwrap_or_else(|| ToJs::Msg("Channel closed".to_owned())))
}

#[op2(async)]
#[string]
async fn op_read_file(#[string] path: String) -> Result<String> {
    let contents = tokio::fs::read_to_string(path).await?;
    Ok(contents)
}

#[op2(async)]
async fn op_write_file(
    #[string] path: String,
    #[string] contents: String,
) -> Result<()> {
    tokio::fs::write(path, contents).await?;
    Ok(())
}

#[op2(fast)]
fn op_remove_file(#[string] path: String) -> Result<()> {
    std::fs::remove_file(path)?;
    Ok(())
}

#[op2(async)]
#[string]
async fn op_fetch(#[string] url: String) -> Result<String> {
    let body = reqwest::get(url).await?.text().await?;
    Ok(body)
}

struct TsModuleLoader;
impl deno_core::ModuleLoader for TsModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: deno_core::ResolutionKind,
    ) -> Result<deno_core::ModuleSpecifier, anyhow::Error> {
        deno_core::resolve_import(specifier, referrer).map_err(Into::into)
    }

    fn load(
        &self,
        module_specifier: &deno_core::ModuleSpecifier,
        _maybe_referrer: Option<&reqwest::Url>,
        _is_dyn_import: bool,
        _requested_module_type: deno_core::RequestedModuleType,
    ) -> ModuleLoadResponse {
        let module_specifier = module_specifier.clone();

        let module_load = move || {
            let path = module_specifier.to_file_path().unwrap();

            let media_type = MediaType::from_path(&path);
            let (module_type, should_transpile) =
                match MediaType::from_path(&path) {
                    MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                        (deno_core::ModuleType::JavaScript, false)
                    }
                    MediaType::Jsx => (deno_core::ModuleType::JavaScript, true),
                    MediaType::TypeScript
                    | MediaType::Mts
                    | MediaType::Cts
                    | MediaType::Dts
                    | MediaType::Dmts
                    | MediaType::Dcts
                    | MediaType::Tsx => {
                        (deno_core::ModuleType::JavaScript, true)
                    }
                    MediaType::Json => (deno_core::ModuleType::Json, false),
                    _ => panic!("Unknown extension {:?}", path.extension()),
                };

            let code = std::fs::read_to_string(&path)?;
            let code = if should_transpile {
                let parsed = deno_ast::parse_module(ParseParams {
                    specifier: module_specifier.clone(),
                    text: code.into(),
                    media_type,
                    capture_tokens: false,
                    scope_analysis: false,
                    maybe_syntax: None,
                })?;
                parsed
                    .transpile(
                        &Default::default(),
                        &Default::default(),
                        &Default::default(),
                    )?
                    .into_source()
                    .text
                    .as_bytes()
                    .to_owned()
            } else {
                code.into_bytes()
            };
            let module = deno_core::ModuleSource::new(
                module_type,
                ModuleSourceCode::Bytes(code.into_boxed_slice().into()),
                &module_specifier,
                None,
            );
            Ok(module)
        };

        ModuleLoadResponse::Sync(module_load())
    }
}

pub fn maybe_transpile_source(
    name: ModuleName,
    source: ModuleCodeString,
) -> Result<(ModuleCodeString, Option<SourceMapData>)> {
    // Always transpile `node:` built-in modules, since they might be
    // TypeScript.
    let media_type = if name.starts_with("node:") {
        MediaType::TypeScript
    } else {
        MediaType::from_path(Path::new(&name))
    };

    match media_type {
        MediaType::TypeScript => {}
        MediaType::JavaScript => return Ok((source, None)),
        MediaType::Mjs => return Ok((source, None)),
        _ => panic!(
            "Unsupported media type for snapshotting {media_type:?} for file \
             {}",
            name
        ),
    }

    let parsed = deno_ast::parse_module(ParseParams {
        specifier: deno_core::url::Url::parse(&name).unwrap(),
        text: source.into(),
        media_type,
        capture_tokens: false,
        scope_analysis: false,
        maybe_syntax: None,
    })?;
    let transpiled_source = parsed
        .transpile(
            &deno_ast::TranspileOptions {
                imports_not_used_as_values:
                    deno_ast::ImportsNotUsedAsValues::Remove,
                ..Default::default()
            },
            &deno_ast::TranspileModuleOptions::default(),
            &deno_ast::EmitOptions {
                source_map: if cfg!(debug_assertions) {
                    SourceMapOption::Separate
                } else {
                    SourceMapOption::None
                },
                ..Default::default()
            },
        )?
        .into_source();

    let maybe_source_map: Option<SourceMapData> = transpiled_source
        .source_map
        .map(|sm| sm.into_bytes().into());
    let source_text = transpiled_source.text;
    Ok((source_text.into(), maybe_source_map))
}
