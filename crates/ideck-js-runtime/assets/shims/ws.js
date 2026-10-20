// Minimal `ws` shim: Node plugins `require('ws')` and often `class X extends
// WebSocket` or read `.Server`. The runtime has a global WebSocket, so wrap it
// in an extendable class and expose the usual statics.
// The embedded runtime may not define a global WebSocket; `extends undefined`
// would throw at load time, so fall back to an inert base class.
const Base =
  globalThis.WebSocket ||
  class {
    constructor() {
      this.__ideckNoop = true;
    }
    addEventListener() {}
    removeEventListener() {}
    send() {}
    close() {}
  };

class WebSocket extends Base {
  constructor(address, protocols, options) {
    super(address, protocols);
    this._options = options || {};
  }
  on(event, listener) {
    super.addEventListener(event, (event) => listener(event?.data ?? event));
    return this;
  }
  once(event, listener) {
    super.addEventListener(event, (event) => listener(event?.data ?? event), { once: true });
    return this;
  }
  off(event, listener) {
    super.removeEventListener(event, listener);
    return this;
  }
  send(data) {
    super.send(data);
    return true;
  }
  close() {
    super.close();
  }
}

class WebSocketServer {
  constructor(options) {
    this.options = options || {};
    this.clients = new Set();
  }
  on() {
    return this;
  }
  close() {}
  handleUpgrade() {}
}

WebSocket.WebSocket = WebSocket;
WebSocket.Server = WebSocketServer;
WebSocket.WebSocketServer = WebSocketServer;
WebSocket.default = WebSocket;

export default WebSocket;
export { WebSocket, WebSocketServer };
export const Server = WebSocketServer;