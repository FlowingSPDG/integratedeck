export function pathToFileURL(path) {
  return globalThis.__ideckFs.toFileUrl(String(path))
}

export default { pathToFileURL }
