export function join(...parts) {
  const cleaned = parts
    .flatMap((p) => String(p).replace(/\\/g, '/').split('/'))
    .filter((p, i) => p.length > 0 || i === 0)
  if (cleaned.length === 0) return '.'
  if (/^[A-Za-z]:$/.test(cleaned[0])) {
    return cleaned.join('/').replace(/\//g, '\\')
  }
  return cleaned.join('/')
}

export function dirname(path) {
  const normalized = String(path).replace(/\\/g, '/')
  const idx = normalized.lastIndexOf('/')
  if (idx <= 0) return normalized.startsWith('/') ? '/' : '.'
  return normalized.slice(0, idx)
}

export function resolve(...parts) {
  return join(...parts)
}

export default { join, dirname, resolve }
