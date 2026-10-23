// Minimal `node:os` shim for the embedded JS runtime.

const platform = () => "win32";

export default {
  platform,
  arch: () => "x64",
  type: () => "Windows_NT",
  release: () => "10.0.0",
  version: () => "",
  EOL: "\r\n",
  homedir: () => ".",
  tmpdir: () => ".",
  hostname: () => "integratedeck",
  cpus: () => [{ model: "generic", speed: 0, times: {} }],
  totalmem: () => 0,
  freemem: () => 0,
  networkInterfaces: () => ({}),
  userInfo: () => ({ username: "integratedeck", homedir: ".", shell: null }),
};

export { platform };
export const EOL = "\r\n";
export const homedir = () => ".";
export const tmpdir = () => ".";
export const arch = () => "x64";
export const hostname = () => "integratedeck";
