use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use boa_engine::native_function::NativeFunction;
use boa_engine::property::Attribute;
use boa_engine::{Context, JsArgs, JsObject, JsResult, JsString, JsValue, Source, js_string};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::warn;

type WsCallback = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone)]
pub enum WsJsEvent {
    Open { id: u64, data: String },
    Message { id: u64, data: String },
    Error { id: u64, data: String },
    Close { id: u64, data: String },
}

struct WsConnection {
    cmd_tx: mpsc::UnboundedSender<Message>,
}

enum WsCommand {
    Connect {
        id: u64,
        url: String,
        on_open: WsCallback,
        on_message: WsCallback,
        on_error: WsCallback,
        on_close: WsCallback,
    },
    Send { id: u64, data: String },
    Close { id: u64 },
}

pub struct WsBridge {
    cmd_tx: mpsc::UnboundedSender<WsCommand>,
    event_tx: mpsc::UnboundedSender<WsJsEvent>,
}

impl WsBridge {
    pub fn spawn() -> (Self, mpsc::UnboundedReceiver<WsJsEvent>) {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            let mut connections: HashMap<u64, WsConnection> = HashMap::new();
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    WsCommand::Connect {
                        id,
                        url,
                        on_open,
                        on_message,
                        on_error,
                        on_close,
                    } => {
                        let (ws_cmd_tx, mut ws_cmd_rx) = mpsc::unbounded_channel();
                        connections.insert(id, WsConnection { cmd_tx: ws_cmd_tx });
                        tokio::spawn(async move {
                            match connect_async(&url).await {
                                Ok((ws, _)) => {
                                    on_open(String::new());
                                    let (mut sink, mut stream) = ws.split();
                                    loop {
                                        tokio::select! {
                                            Some(cmd) = ws_cmd_rx.recv() => {
                                                match cmd {
                                                    Message::Text(text) => {
                                                        if sink.send(Message::Text(text)).await.is_err() {
                                                            break;
                                                        }
                                                    }
                                                    Message::Close(_) => {
                                                        let _ = sink.close().await;
                                                        break;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                            msg = stream.next() => {
                                                match msg {
                                                    Some(Ok(Message::Text(text))) => on_message(text.to_string()),
                                                    Some(Ok(Message::Close(_))) | None => break,
                                                    Some(Err(e)) => {
                                                        on_error(e.to_string());
                                                        break;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                    on_close(String::new());
                                }
                                Err(e) => on_error(e.to_string()),
                            }
                        });
                    }
                    WsCommand::Send { id, data } => {
                        if let Some(conn) = connections.get(&id) {
                            let _ = conn.cmd_tx.send(Message::Text(data.into()));
                        }
                    }
                    WsCommand::Close { id } => {
                        if let Some(conn) = connections.remove(&id) {
                            let _ = conn.cmd_tx.send(Message::Close(None));
                        }
                    }
                }
            }
        });

        (
            Self { cmd_tx, event_tx },
            event_rx,
        )
    }

    fn emit(&self, event: WsJsEvent) {
        let _ = self.event_tx.send(event);
    }
}

pub struct WsCallbacks {
    pub on_open: JsObject,
    pub on_message: JsObject,
    pub on_error: JsObject,
    pub on_close: JsObject,
}

struct WsInstallState {
    bridge: Arc<WsBridge>,
    callbacks: Rc<RefCell<HashMap<u64, WsCallbacks>>>,
    next_id: RefCell<u64>,
}

thread_local! {
    static WS_STATE: RefCell<Option<WsInstallState>> = const { RefCell::new(None) };
}

pub fn install_websocket(
    context: &mut Context,
    bridge: Arc<WsBridge>,
    callbacks: Rc<RefCell<HashMap<u64, WsCallbacks>>>,
) -> JsResult<()> {
    WS_STATE.with(|slot| {
        *slot.borrow_mut() = Some(WsInstallState {
            bridge,
            callbacks,
            next_id: RefCell::new(0),
        });
    });

    let ws_api = JsObject::with_null_proto();
    ws_api.set(
        js_string!("create"),
        NativeFunction::from_fn_ptr(ws_create).to_js_function(context.realm()),
        false,
        context,
    )?;
    ws_api.set(
        js_string!("send"),
        NativeFunction::from_fn_ptr(ws_send).to_js_function(context.realm()),
        false,
        context,
    )?;
    ws_api.set(
        js_string!("close"),
        NativeFunction::from_fn_ptr(ws_close).to_js_function(context.realm()),
        false,
        context,
    )?;

    context.register_global_property(
        js_string!("__ideckWs"),
        ws_api,
        Attribute::all(),
    )?;

    context.eval(Source::from_bytes(WEBSOCKET_INSTALL))?;
    Ok(())
}

fn ws_create(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    WS_STATE.with(|slot| {
        let binding = slot.borrow();
        let Some(state) = binding.as_ref() else {
            return Err(boa_engine::JsNativeError::typ()
                .with_message("websocket bridge not installed")
                .into());
        };

        let url = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let on_open: JsObject = args
            .get_or_undefined(1)
            .as_function()
            .ok_or_else(|| boa_engine::JsNativeError::typ().with_message("on_open required"))?
            .into();
        let on_message: JsObject = args
            .get_or_undefined(2)
            .as_function()
            .ok_or_else(|| boa_engine::JsNativeError::typ().with_message("on_message required"))?
            .into();
        let on_error: JsObject = args
            .get_or_undefined(3)
            .as_function()
            .ok_or_else(|| boa_engine::JsNativeError::typ().with_message("on_error required"))?
            .into();
        let on_close: JsObject = args
            .get_or_undefined(4)
            .as_function()
            .ok_or_else(|| boa_engine::JsNativeError::typ().with_message("on_close required"))?
            .into();

        let id = {
            let mut guard = state.next_id.borrow_mut();
            *guard += 1;
            *guard
        };

        state.callbacks.borrow_mut().insert(
            id,
            WsCallbacks {
                on_open,
                on_message,
                on_error,
                on_close,
            },
        );

        let on_open_cb: WsCallback = Arc::new({
            let bridge = state.bridge.clone();
            move |data| {
                bridge.emit(WsJsEvent::Open {
                    id,
                    data: data.clone(),
                });
            }
        });
        let on_message_cb: WsCallback = Arc::new({
            let bridge = state.bridge.clone();
            move |data| {
                bridge.emit(WsJsEvent::Message {
                    id,
                    data: data.clone(),
                });
            }
        });
        let on_error_cb: WsCallback = Arc::new({
            let bridge = state.bridge.clone();
            move |data| {
                bridge.emit(WsJsEvent::Error {
                    id,
                    data: data.clone(),
                });
            }
        });
        let on_close_cb: WsCallback = Arc::new({
            let bridge = state.bridge.clone();
            move |data| {
                bridge.emit(WsJsEvent::Close {
                    id,
                    data: data.clone(),
                });
            }
        });

        let _ = state.bridge.cmd_tx.send(WsCommand::Connect {
            id,
            url,
            on_open: on_open_cb,
            on_message: on_message_cb,
            on_error: on_error_cb,
            on_close: on_close_cb,
        });

        Ok(JsValue::from(id))
    })
}

fn ws_send(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    WS_STATE.with(|slot| {
        let binding = slot.borrow();
        let Some(state) = binding.as_ref() else {
            return Ok(JsValue::undefined());
        };
        let id = args.get_or_undefined(0).to_number(ctx)? as u64;
        let data = args
            .get_or_undefined(1)
            .to_string(ctx)?
            .to_std_string_escaped();
        let _ = state
            .bridge
            .cmd_tx
            .send(WsCommand::Send { id, data });
        Ok(JsValue::undefined())
    })
}

fn ws_close(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    WS_STATE.with(|slot| {
        let binding = slot.borrow();
        let Some(state) = binding.as_ref() else {
            return Ok(JsValue::undefined());
        };
        let id = args.get_or_undefined(0).to_number(ctx)? as u64;
        let _ = state.bridge.cmd_tx.send(WsCommand::Close { id });
        Ok(JsValue::undefined())
    })
}

const WEBSOCKET_INSTALL: &str = r#"
(function () {
  const READY = 0;
  const OPEN = 1;
  const CLOSED = 3;

  globalThis.WebSocket = class WebSocket {
    constructor(url) {
      this.url = String(url);
      this.readyState = READY;
      this._listeners = { open: [], message: [], error: [], close: [] };
      this._id = null;

      const self = this;
      this._id = globalThis.__ideckWs.create(
        this.url,
        () => {
          self.readyState = OPEN;
          self._emit('open', {});
        },
        (data) => {
          self._emit('message', { data });
        },
        (err) => {
          self._emit('error', { message: err });
        },
        () => {
          self.readyState = CLOSED;
          self._emit('close', {});
        },
      );
    }

    addEventListener(type, listener) {
      const key = String(type);
      if (!this._listeners[key]) this._listeners[key] = [];
      this._listeners[key].push(listener);
    }

    removeEventListener(type, listener) {
      const key = String(type);
      const list = this._listeners[key];
      if (!list) return;
      const idx = list.indexOf(listener);
      if (idx >= 0) list.splice(idx, 1);
    }

    send(data) {
      if (this._id == null) return;
      globalThis.__ideckWs.send(this._id, String(data));
    }

    close() {
      if (this._id == null) return;
      globalThis.__ideckWs.close(this._id);
      this._id = null;
      this.readyState = CLOSED;
    }

    _emit(type, event) {
      const list = this._listeners[type] ?? [];
      for (const listener of list) {
        try {
          listener.call(this, event);
        } catch (_) {}
      }
    }
  };
})();
"#;

pub fn dispatch_ws_event(
    context: &mut Context,
    callbacks: &Rc<RefCell<HashMap<u64, WsCallbacks>>>,
    event: WsJsEvent,
) {
    let (id, data) = match &event {
        WsJsEvent::Open { id, data } => (*id, data.clone()),
        WsJsEvent::Message { id, data } => (*id, data.clone()),
        WsJsEvent::Error { id, data } => (*id, data.clone()),
        WsJsEvent::Close { id, data } => (*id, data.clone()),
    };

    let guard = callbacks.borrow();
    let Some(cbs) = guard.get(&id) else {
        return;
    };

    let obj = match event {
        WsJsEvent::Open { .. } => &cbs.on_open,
        WsJsEvent::Message { .. } => &cbs.on_message,
        WsJsEvent::Error { .. } => &cbs.on_error,
        WsJsEvent::Close { .. } => &cbs.on_close,
    };

    let value: JsValue = obj.clone().into();
    let Some(callable) = value.as_callable() else {
        return;
    };

    if let Err(e) = callable.call(
        &JsValue::undefined(),
        &[JsValue::from(JsString::from(data))],
        context,
    ) {
        warn!("websocket callback failed: {e}");
    }
}
