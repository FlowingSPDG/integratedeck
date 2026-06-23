use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use boa_engine::{Context, JsValue, Source, js_string};
use boa_runtime::extensions::{ConsoleExtension, MicrotaskExtension, TimeoutExtension};
use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, oneshot};
use tracing::error;

use crate::loader::IdeckModuleLoader;
use crate::shims::{install_globals, install_require};
use crate::ws::{dispatch_ws_event, install_websocket, WsBridge};

pub struct JsEngineConfig {
    pub module_root: PathBuf,
    pub bootstrap: &'static str,
    pub thread_name: &'static str,
    pub with_websocket: bool,
    pub event_emitter: Option<Arc<dyn Fn(String, String) + Send + Sync>>,
    pub startup_call: Option<(String, JsonValue)>,
}

enum WorkerRequest {
    Call {
        function_name: String,
        params: JsonValue,
        reply: oneshot::Sender<anyhow::Result<JsonValue>>,
    },
    Shutdown,
}

pub struct JsEngine {
    request_tx: mpsc::UnboundedSender<WorkerRequest>,
}

impl JsEngine {
    pub fn spawn(config: JsEngineConfig) -> anyhow::Result<Self> {
        let (request_tx, request_rx) = mpsc::unbounded_channel();

        std::thread::Builder::new()
            .name(config.thread_name.to_string())
            .spawn(move || {
                if let Err(e) = run_worker(config, request_rx) {
                    error!("JS engine thread failed: {e:#}");
                }
            })?;

        Ok(Self { request_tx })
    }

    pub async fn call(
        &self,
        function_name: impl Into<String>,
        params: JsonValue,
    ) -> anyhow::Result<JsonValue> {
        let (tx, rx) = oneshot::channel();
        self.request_tx.send(WorkerRequest::Call {
            function_name: function_name.into(),
            params,
            reply: tx,
        })?;
        rx.await
            .map_err(|_| anyhow::anyhow!("JS engine response channel closed"))?
    }
}

impl Drop for JsEngine {
    fn drop(&mut self) {
        let _ = self.request_tx.send(WorkerRequest::Shutdown);
    }
}

fn run_worker(
    config: JsEngineConfig,
    mut request_rx: mpsc::UnboundedReceiver<WorkerRequest>,
) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async move {
        let ws_callbacks = Arc::new(Mutex::new(std::collections::HashMap::<
            u64,
            crate::ws::WsCallbacks,
        >::new()));
        let mut ws_event_rx = if config.with_websocket {
            let (bridge, rx) = WsBridge::spawn();
            Some((Arc::new(bridge), rx))
        } else {
            None
        };

        let loader = Rc::new(IdeckModuleLoader::new(config.module_root.clone()));
        let loader_for_require = loader.clone();
        let mut context = Context::builder()
            .module_loader(loader)
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build JS context: {e}"))?;

        boa_runtime::register(
            ConsoleExtension::default(),
            None,
            &mut context,
        )
        .map_err(|e| anyhow::anyhow!("failed to register console extension: {e}"))?;
        boa_runtime::register_extensions(MicrotaskExtension {}, None, &mut context)
            .map_err(|e| anyhow::anyhow!("failed to register microtask extension: {e}"))?;
        boa_runtime::register_extensions(TimeoutExtension {}, None, &mut context)
            .map_err(|e| anyhow::anyhow!("failed to register timeout extension: {e}"))?;

        install_globals(&mut context, config.event_emitter.clone())
            .map_err(|e| anyhow::anyhow!("failed to install globals: {e}"))?;

        install_require(&mut context, loader_for_require)
            .map_err(|e| anyhow::anyhow!("failed to install require: {e}"))?;

        if let Some((bridge, _)) = &ws_event_rx {
            install_websocket(&mut context, bridge.clone(), ws_callbacks.clone())
                .map_err(|e| anyhow::anyhow!("failed to install websocket: {e}"))?;
        }

        context
            .eval(Source::from_bytes(config.bootstrap.as_bytes()))
            .map_err(|e| anyhow::anyhow!("bootstrap eval failed: {e}"))?;
        context
            .run_jobs()
            .map_err(|e| anyhow::anyhow!("bootstrap jobs failed: {e}"))?;

        if let Some((function_name, params)) = config.startup_call {
            if let Err(e) = handle_call(&mut context, &function_name, params) {
                error!("JS engine startup call failed: {e:#}");
            }
        }

        loop {
            if let Some((_, rx)) = &mut ws_event_rx {
                while let Ok(event) = rx.try_recv() {
                    dispatch_ws_event(&mut context, &ws_callbacks, event);
                    let _ = context.run_jobs();
                }
            }

            let next = if let Some((_, rx)) = &mut ws_event_rx {
                tokio::select! {
                    req = request_rx.recv() => req,
                    event = rx.recv() => {
                        if let Some(event) = event {
                            dispatch_ws_event(&mut context, &ws_callbacks, event);
                            let _ = context.run_jobs();
                        }
                        continue;
                    }
                }
            } else {
                request_rx.recv().await
            };

            match next {
                Some(WorkerRequest::Call {
                    function_name,
                    params,
                    reply,
                }) => {
                    let response = handle_call(&mut context, &function_name, params);
                    let _ = reply.send(response);
                }
                Some(WorkerRequest::Shutdown) | None => break,
            }
        }
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}

fn handle_call(
    context: &mut Context,
    function_name: &str,
    params: JsonValue,
) -> anyhow::Result<JsonValue> {
    let global = context.global_object();
    let handle = global
        .get(js_string!(function_name), context)
        .map_err(js_error)?;
    let callable = handle
        .as_callable()
        .ok_or_else(|| anyhow::anyhow!("{function_name} is not callable"))?;

    let args: Vec<JsValue> = match params {
        JsonValue::Array(items) => items
            .into_iter()
            .map(|item| json_to_js(item, context))
            .collect::<Result<_, _>>()?,
        other => vec![json_to_js(other, context)?],
    };

    let result = callable
        .call(&JsValue::undefined(), &args, context)
        .map_err(js_error)?;

    let value = if let Some(promise) = result.as_promise() {
        promise.await_blocking(context).map_err(js_error)?
    } else {
        result
    };

    context
        .run_jobs()
        .map_err(|e| anyhow::anyhow!("run_jobs failed: {e}"))?;

    js_to_json(value, context)
}

fn json_to_js(json: JsonValue, context: &mut Context) -> anyhow::Result<JsValue> {
    let encoded = serde_json::to_string(&json)?;
    let script = format!("JSON.parse({encoded:?})");
    context
        .eval(Source::from_bytes(script.as_bytes()))
        .map_err(|e| anyhow::anyhow!("json parse failed: {e}"))
}

fn js_to_json(value: JsValue, context: &mut Context) -> anyhow::Result<JsonValue> {
    if value.is_undefined() {
        return Ok(JsonValue::Null);
    }
    let json_obj = context
        .global_object()
        .get(js_string!("JSON"), context)
        .map_err(js_error)?;
    let stringify = json_obj
        .as_object()
        .and_then(|o| o.get(js_string!("stringify"), context).ok())
        .and_then(|v| v.as_callable())
        .ok_or_else(|| anyhow::anyhow!("JSON.stringify unavailable"))?;
    let text = stringify
        .call(&json_obj, &[value], context)
        .map_err(js_error)?;
    let s = text
        .to_string(context)
        .map_err(js_error)?
        .to_std_string_escaped();
    serde_json::from_str(&s).map_err(|e| anyhow::anyhow!("invalid JSON from JS: {e}"))
}

fn js_error(err: boa_engine::JsError) -> anyhow::Error {
    anyhow::anyhow!("{err}")
}
