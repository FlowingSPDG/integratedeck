export function existsSync(path) {
  return globalThis.__ideckFs.existsSync(String(path))
}

export function readFileSync(path, _encoding) {
  return globalThis.__ideckFs.readFileSync(String(path))
}

export default { existsSync, readFileSync }
