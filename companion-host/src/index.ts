/**
 * Orchestrator-managed companion module host (stdio JSON-RPC + events).
 */

import * as readline from 'node:readline'
import { stdin, stdout } from 'node:process'
import { pathToFileURL } from 'node:url'
import { join } from 'node:path'
import { existsSync } from 'node:fs'

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

interface ConnectionState {
  moduleId: string
  modulePath: string
  config: unknown
  instance: unknown | null
}

const connections = new Map<string, ConnectionState>()
const actionDefinitions = new Map<string, unknown[]>()
const feedbackDefinitions = new Map<string, unknown[]>()
const variableDefinitions = new Map<string, unknown[]>()
const variableValues = new Map<string, Map<string, string>>()

function emit(event: string, data: Record<string, unknown>): void {
  stdout.write(JSON.stringify({ event, ...data }) + '\n')
}

function respond(res: Response): void {
  stdout.write(JSON.stringify(res) + '\n')
}

function modulesRoot(): string {
  const env = process.env.INTEGRATEDECK_COMPANION_MODULES
  if (env && existsSync(env)) return env
  const home = process.env.APPDATA ?? process.env.HOME ?? '.'
  return join(home, 'integratedeck', 'plugins', 'companion')
}

async function loadModuleEntry(modulePath: string): Promise<unknown> {
  const entry = join(modulePath, 'dist', 'index.js')
  const alt = join(modulePath, 'index.js')
  const file = existsSync(entry) ? entry : alt
  if (!existsSync(file)) {
    throw new Error(`module entry not found: ${modulePath}`)
  }
  return import(pathToFileURL(file).href)
}

async function initConnection(id: string, moduleId: string, config: unknown): Promise<void> {
  const root = modulesRoot()
  const modulePath = join(root, moduleId)
  if (!existsSync(modulePath)) {
    throw new Error(`module directory not found: ${modulePath}`)
  }
  const mod = await loadModuleEntry(modulePath)
  const InstanceClass = (mod as { default?: new () => unknown }).default
  if (!InstanceClass) {
    throw new Error(`module ${moduleId} has no default export`)
  }
  const instance = new InstanceClass()
  connections.set(id, { moduleId, modulePath, config, instance })
  emit('connection.status', { connectionId: id, status: 'connected', moduleId })
}

async function handle(req: Request): Promise<Response> {
  const base = { id: req.id, ok: true as const }
  try {
    switch (req.method) {
      case 'ping':
        return {
          ...base,
          result: {
            pong: true,
            node: process.version,
            connections: connections.size,
            modulesRoot: modulesRoot(),
          },
        }

      case 'connection.add': {
        const { id, moduleId, config } = req.params as {
          id: string
          moduleId: string
          config: unknown
        }
        await initConnection(id, moduleId, config)
        return { ...base, result: { id } }
      }

      case 'connection.remove': {
        const { connectionId } = req.params as { connectionId: string }
        connections.delete(connectionId)
        actionDefinitions.delete(connectionId)
        feedbackDefinitions.delete(connectionId)
        variableDefinitions.delete(connectionId)
        variableValues.delete(connectionId)
        emit('connection.status', { connectionId, status: 'disconnected' })
        return { ...base, result: { connectionId } }
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
        const inst = conn.instance as {
          executeAction?: (action: { id: string; options: unknown }) => Promise<void>
        }
        if (typeof inst?.executeAction === 'function') {
          await inst.executeAction({ id: actionId, options: options ?? {} })
        }
        return { ...base, result: { executed: true, connectionId, actionId } }
      }

      case 'connection.getDefinitions': {
        const { connectionId } = req.params as { connectionId: string }
        return {
          ...base,
          result: {
            actions: actionDefinitions.get(connectionId) ?? [],
            feedbacks: feedbackDefinitions.get(connectionId) ?? [],
            variables: variableDefinitions.get(connectionId) ?? [],
          },
        }
      }

      case 'connection.setActionDefinitions': {
        const { connectionId, definitions } = req.params as {
          connectionId: string
          definitions: unknown[]
        }
        actionDefinitions.set(connectionId, definitions)
        emit('definitions.updated', { connectionId, kind: 'actions', definitions })
        return { ...base, result: {} }
      }

      case 'connection.updateFeedbackValues': {
        const { connectionId, values } = req.params as {
          connectionId: string
          values: unknown[]
        }
        emit('feedback.updated', { connectionId, values })
        return { ...base, result: {} }
      }

      case 'connection.setVariableValues': {
        const { connectionId, values } = req.params as {
          connectionId: string
          values: { id: string; value: string }[]
        }
        const map = variableValues.get(connectionId) ?? new Map()
        for (const v of values) {
          map.set(v.id, v.value)
        }
        variableValues.set(connectionId, map)
        emit('variable.updated', { connectionId, values })
        return { ...base, result: {} }
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

stdout.write(JSON.stringify({ event: 'ready', version: '0.1.0' }) + '\n')
