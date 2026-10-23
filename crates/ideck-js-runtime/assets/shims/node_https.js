// Minimal `node:https` / `node:http` shim for the embedded JS runtime.
//
// Plugins commonly require these for update checks or webhooks. The embedded
// runtime has no sockets, so the module must load (require must not throw) while
// any actual request fails loudly instead of silently hanging.

function unsupported(name) {
  return function () {
    throw new Error(
      "integratedeck: '" + name + "' is not supported in the embedded JS runtime",
    );
  };
}

const notSupported = {
  request: unsupported("http(s).request"),
  get: unsupported("http(s).get"),
  createServer: unsupported("http(s).createServer"),
  Server: unsupported("http(s).Server"),
};

export default {
  ...notSupported,
  Agent: function Agent() {},
  STATUS_CODES: {},
  METHODS: [],
  globalAgent: {},
};

export const request = notSupported.request;
export const get = notSupported.get;
export const createServer = notSupported.createServer;
export const STATUS_CODES = {};
export const METHODS = [];
export const globalAgent = {};
