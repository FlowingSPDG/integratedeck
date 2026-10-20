// Catch-all shim for Node built-ins the embedded JS runtime cannot provide.
//
// Plugin code often requires modules it only uses on optional paths (update
// checks, telemetry, deep library imports). Those requires must not throw while
// loading, otherwise the whole plugin fails to start. Using the module throws a
// clear error that names it instead of failing silently.

const MODULE_NAME = "__IDECK_MODULE_NAME__";

function unsupported(method) {
  return function () {
    throw new Error(
      "integratedeck: '" +
        MODULE_NAME +
        "." +
        method +
        "' is not supported in the embedded JS runtime",
    );
  };
}

const METHODS = [
  "request",
  "get",
  "createServer",
  "createConnection",
  "connect",
  "createHash",
  "createHmac",
  "createCipheriv",
  "createDecipheriv",
  "randomBytes",
  "randomUUID",
  "pbkdf2",
  "scrypt",
  "createReadStream",
  "createWriteStream",
  "readFile",
  "writeFile",
  "spawn",
  "exec",
  "execFile",
  "fork",
  "resolve",
  "lookup",
  "format",
  "inspect",
  "promisify",
  "inherits",
  "deflate",
  "inflate",
  "gunzip",
  "gzipSync",
  "pipeline",
];

// Classes must exist as constructors: libraries write
// `class Client extends net.Socket` while loading.
const CLASS_NAMES = [
  "Socket",
  "Server",
  "Agent",
  "ClientRequest",
  "IncomingMessage",
  "TLSSocket",
  "Hash",
  "Hmac",
  "Cipher",
  "Decipher",
  "Transform",
  "Readable",
  "Writable",
  "Duplex",
  "Stream",
];

const shim = { supported: false, moduleName: MODULE_NAME };
for (const method of METHODS) {
  shim[method] = unsupported(method);
}
for (const className of CLASS_NAMES) {
  // Subclasses call super(), so construction must succeed; record the gap on the
  // instance instead of throwing so the rest of the plugin can still start.
  shim[className] = class {
    constructor() {
      this.__unsupported = MODULE_NAME + "." + className;
    }
  };
}

export default shim;
export const supported = false;
export const moduleName = MODULE_NAME;
