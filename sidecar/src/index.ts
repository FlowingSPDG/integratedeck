/**
 * integratedeck Node sidecar — JSON-RPC over stdin/stdout.
 * Wraps Companion module/surface host packages when available.
 */

import * as readline from 'node:readline'
import { stdin, stdout } from 'node:process'

interface Request {
  id: string
  method: string
  params: Record<string, unknown>
}

interface Response {
  id: string
  ok: boolean
  result?: unknown
  error?: string
}

const connections = new Map<string, { moduleId: string; config: unknown }>()

function respond(res: Response): void {
  stdout.write(JSON.stringify(res) + '\n')
}

async function handle(req: Request): Promise<Response> {
  const base = { id: req.id, ok: true as const }
  try {
    switch (req.method) {
      case 'ping': {
        const { tryLoadCompanionHost, tryLoadSurfaceHost } = await import('./companion-host.js')
        const [companion, surface] = await Promise.all([
          tryLoadCompanionHost(),
          tryLoadSurfaceHost(),
        ])
        return {
          ...base,
          result: {
            pong: true,
            node: process.version,
            companionHost: companion,
            surfaceHost: surface,
          },
        }
      }

      case 'connection.add': {
        const { id, moduleId, config } = req.params as {
          id: string
          moduleId: string
          config: unknown
        }
        connections.set(id, { moduleId, config })
        return { ...base, result: { id } }
      }

      case 'connection.executeAction': {
        const { connectionId, actionId, options } = req.params as {
          connectionId: string
          actionId: string
          options: unknown
        }
        const conn = connections.get(connectionId)
        if (!conn) {
          return { id: req.id, ok: false, error: `unknown connection ${connectionId}` }
        }
        // Full @companion-module/host wiring loads module child processes in Phase 4+
        return {
          ...base,
          result: {
            executed: true,
            connectionId,
            actionId,
            options,
            moduleId: conn.moduleId,
          },
        }
      }

      case 'surface.list': {
        return { ...base, result: { surfaces: [] } }
      }

      case 'surface.scan': {
        return { ...base, result: { message: 'surface scan delegated to @companion-surface/host' } }
      }

      default:
        return { id: req.id, ok: false, error: `unknown method: ${req.method}` }
    }
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e)
    return { id: req.id, ok: false, error: message }
  }
}

const rl = readline.createInterface({ input: stdin, crlfDelay: Infinity })

rl.on('line', (line) => {
  if (!line.trim()) return
  void (async () => {
    try {
      const req = JSON.parse(line) as Request
      const res = await handle(req)
      respond(res)
    } catch (e) {
      respond({
        id: 'parse-error',
        ok: false,
        error: e instanceof Error ? e.message : String(e),
      })
    }
  })()
})

stdout.write(
  JSON.stringify({ event: 'ready', version: '0.1.0' }) + '\n',
)
