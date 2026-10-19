// Companion module sidecar: runs real Companion modules under Node so they get
// the actual `@companion-module/base` (including TCPHelper) and Node sockets.
//
// Protocol: one JSON object per line on stdin/stdout.
//   request  { id, method, params }
//   response { id, ok, result?, error? }
//   event    { event, ... }

import { createInterface } from 'node:readline'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const connections = new Map()

function emit(obj) {
  process.stdout.write(`${JSON.stringify(obj)}\n`)
}

function log(level, ...args) {
  emit({ event: 'log', level, message: args.map(String).join(' ') })
}

function makeContext(id, cache) {
  const variableValues = new Map()

  const context = {
    _isInstanceContext: true,
    id,
    label: id,
    upgradeScripts: [],

    setActionDefinitions(definitions) {
      cache.actionDefinitions = definitions ?? {}
      cache.actions = Object.entries(cache.actionDefinitions).map(([actionId, action]) => ({
        id: actionId,
        name: action.name,
        description: action.description,
        options: action.options,
        hasLearn: !!action.learn,
      }))
      emit({ event: 'definitions.updated', connectionId: id, kind: 'actions', definitions: cache.actions })
    },

    setFeedbackDefinitions(definitions) {
      cache.feedbackDefinitions = definitions ?? {}
      cache.feedbacks = Object.entries(cache.feedbackDefinitions).map(([feedbackId, feedback]) => ({
        id: feedbackId,
        name: feedback.name,
        type: feedback.type,
        options: feedback.options,
      }))
      emit({ event: 'definitions.updated', connectionId: id, kind: 'feedbacks', definitions: cache.feedbacks })
    },

    setVariableDefinitions(definitions) {
      cache.variables = Object.entries(definitions ?? {}).map(([variableId, definition]) => ({
        id: variableId,
        name: definition?.name ?? variableId,
      }))
      emit({ event: 'definitions.updated', connectionId: id, kind: 'variables', definitions: cache.variables })
    },

    setVariableValues(values) {
      const updates = []
      for (const [variableId, value] of Object.entries(values ?? {})) {
        variableValues.set(variableId, value ?? '')
        updates.push({ id: variableId, value: value ?? '' })
      }
      emit({ event: 'variable.updated', connectionId: id, values: updates })
    },

    getVariableValue(variableId) {
      return variableValues.get(variableId)
    },

    setPresetDefinitions(presets) {
      cache.presets = Object.values(presets ?? {})
      emit({ event: 'definitions.updated', connectionId: id, kind: 'presets', definitions: cache.presets })
    },

    checkFeedbacks() {},
    checkFeedbacksById() {},
    subscribeActions() {},
    unsubscribeActions() {},
    subscribeFeedbacks() {},
    unsubscribeFeedbacks() {},

    saveConfig(config, secrets) {
      cache.config = config
      cache.secrets = secrets
      emit({ event: 'connection.config', connectionId: id, config, secrets })
    },

    updateStatus(status, message) {
      emit({ event: 'connection.status', connectionId: id, status, message })
    },

    log(level, message) {
      log(level, id, message)
    },

    parseVariablesInString(text) {
      return String(text ?? '').replace(/\$\(([^)]+)\)/g, (match, name) => {
        const value = variableValues.get(name)
        return value === undefined ? match : String(value)
      })
    },

    oscSend() {},
    recordAction() {},
    sharedUdpSocketJoin: async () => '',
    sharedUdpSocketLeave: async () => {},
    sharedUdpSocketSend: async () => {},
  }

  return context
}

async function importModule(moduleDir) {
  const entry = path.join(moduleDir, 'dist', 'index.js')
  const url = `${pathToFileURL(entry).href}?v=${Date.now()}`
  return import(url)
}

async function addConnection(params, modulesRoot) {
  const id = params.id ?? 'default'
  const moduleId = params.moduleId
  if (!moduleId) throw new Error('moduleId is required')
  const moduleDir = path.join(modulesRoot, moduleId)

  const mod = await importModule(moduleDir)
  const InstanceClass = mod.default
  if (typeof InstanceClass !== 'function') {
    throw new Error(`module ${moduleId} has no default export class`)
  }

  const cache = {
    actions: [],
    actionDefinitions: {},
    feedbacks: [],
    feedbackDefinitions: {},
    variables: [],
    presets: [],
    config: params.config ?? {},
    secrets: params.secrets ?? {},
  }
  const context = makeContext(id, cache)
  context.label = params.label ?? moduleId

  const instance = new InstanceClass(context)
  const configFields = typeof instance.getConfigFields === 'function' ? instance.getConfigFields() : []

  await instance.init(params.config ?? {}, true, params.secrets ?? {})

  connections.set(id, { instance, context, cache, moduleId, moduleDir, configFields })
  emit({ event: 'connection.status', connectionId: id, status: 'ok', moduleId })
  return { id, moduleId, configFields, definitions: publicDefinitions(cache) }
}

function publicDefinitions(cache) {
  return {
    actions: cache.actions,
    feedbacks: cache.feedbacks,
    variables: cache.variables,
    presets: cache.presets,
  }
}

async function executeAction(params) {
  const connection = connections.get(params.connectionId)
  if (!connection) throw new Error(`unknown connection ${params.connectionId}`)
  const definition = connection.cache.actionDefinitions[params.actionId]
  if (!definition?.callback) throw new Error(`unknown action ${params.actionId}`)
  const result = await definition.callback(
    {
      id: `action-${params.actionId}`,
      actionId: params.actionId,
      options: params.options ?? {},
      controlId: `${params.connectionId}:${params.actionId}`,
    },
    {},
  )
  return { executed: true, result: result ?? null }
}

async function handle(method, params, modulesRoot) {
  switch (method) {
    case 'ping':
      return { pong: true, engine: 'node-sidecar', connections: connections.size }
    case 'connection.add':
      return addConnection(params, modulesRoot)
    case 'connection.remove': {
      const connection = connections.get(params.connectionId)
      if (connection?.instance?.destroy) await connection.instance.destroy()
      connections.delete(params.connectionId)
      return { connectionId: params.connectionId }
    }
    case 'connection.getDefinitions': {
      const connection = connections.get(params.connectionId)
      if (!connection) return { actions: [], feedbacks: [], variables: [], presets: [] }
      return publicDefinitions(connection.cache)
    }
    case 'connection.getConfigFields': {
      const connection = connections.get(params.connectionId)
      if (!connection) throw new Error(`unknown connection ${params.connectionId}`)
      return { fields: connection.configFields }
    }
    case 'connection.executeAction':
      return executeAction(params)
    case 'connection.getVariableValue': {
      const connection = connections.get(params.connectionId)
      if (!connection) throw new Error(`unknown connection ${params.connectionId}`)
      return { value: connection.context.getVariableValue(params.variableId) }
    }
    case 'connection.setVariableValue': {
      const connection = connections.get(params.connectionId)
      if (!connection) throw new Error(`unknown connection ${params.connectionId}`)
      connection.context.setVariableValues({ [params.variableId]: params.value })
      return { connectionId: params.connectionId, variableId: params.variableId, value: params.value }
    }
    case 'connection.updateConfig': {
      const connection = connections.get(params.connectionId)
      if (!connection) throw new Error(`unknown connection ${params.connectionId}`)
      if (connection.instance.configUpdated) {
        await connection.instance.configUpdated(params.config ?? {}, params.secrets ?? {})
      }
      connection.cache.config = params.config ?? connection.cache.config
      return { connectionId: params.connectionId }
    }
    default:
      throw new Error(`unknown method: ${method}`)
  }
}

const demoIndex = process.argv.indexOf('--demo')
if (demoIndex !== -1) {
  const moduleDir = process.argv[demoIndex + 1]
  const result = await addConnection(
    { id: 'demo', moduleId: path.basename(moduleDir), config: {} },
    path.dirname(moduleDir),
  )
  console.log(JSON.stringify(result, null, 2))
  process.exit(0)
}

const modulesRoot = process.argv[2] ?? process.cwd()
const rl = createInterface({ input: process.stdin, crlfDelay: Infinity })
rl.on('line', async (line) => {
  const text = line.trim()
  if (!text) return
  let request
  try {
    request = JSON.parse(text)
  } catch (error) {
    emit({ id: null, ok: false, error: `invalid json: ${error}` })
    return
  }
  try {
    const result = await handle(request.method, request.params ?? {}, modulesRoot)
    emit({ id: request.id, ok: true, result })
  } catch (error) {
    emit({ id: request.id, ok: false, error: String(error?.message ?? error) })
  }
})
