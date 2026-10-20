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

const shim = { supported: false, moduleName: MODULE_NAME };
for (const method of METHODS) {
  shim[method] = unsupported(method);
}

export default shim;
export const supported = false;
export const moduleName = MODULE_NAME;
