// Minimal `node:process` shim for the embedded JS runtime.

const env = {};

export default {
  env,
  platform: "win32",
  arch: "x64",
  versions: { node: "20.0.0", v8: "0.0.0" },
  argv: ["node", "plugin.js"],
  execPath: "node",
  pid: 1,
  cwd: () => ".",
  chdir: () => {},
  nextTick: (fn) => {
    Promise.resolve().then(fn);
  },
  exit: (code) => {
    throw new Error("integratedeck: process.exit(" + (code ?? 0) + ") called");
  },
  on: () => {},
  once: () => {},
  off: () => {},
  emit: () => false,
  hrtime: () => [0, 0],
  uptime: () => 0,
  memoryUsage: () => ({ rss: 0, heapTotal: 0, heapUsed: 0, external: 0 }),
  stdout: { write: () => true },
  stderr: { write: () => true },
};

export { env };
export const platform = "win32";
export const arch = "x64";
export const cwd = () => ".";
export const nextTick = (fn) => Promise.resolve().then(fn);
export const argv = ["node", "plugin.js"];
export const versions = { node: "20.0.0" };
