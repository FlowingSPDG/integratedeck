/**
 * Optional Companion module host wiring via @companion-module/host.
 * Loaded dynamically so the sidecar still runs when the package is unavailable.
 */

export async function tryLoadCompanionHost(): Promise<{
  available: boolean
  version?: string
}> {
  try {
    const mod = await import('@companion-module/host')
    return { available: true, version: String((mod as { version?: string }).version ?? 'loaded') }
  } catch {
    return { available: false }
  }
}

export async function tryLoadSurfaceHost(): Promise<{ available: boolean }> {
  try {
    await import('@companion-surface/host')
    return { available: true }
  } catch {
    return { available: false }
  }
}
