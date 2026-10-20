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

// Companion substitutes `$(var)` expressions in option values before a module
// sees them; without it a module referencing a variable receives the literal.
function substituteExpressions(value, variables) {
  if (!variables) return value
  if (typeof value === 'string') {
    return value.replace(/\$\(([^)]+)\)/g, (match, name) => {
      const resolved = variables.get(String(name).trim())
      return resolved === undefined ? match : String(resolved)
    })
  }
  if (Array.isArray(value)) {
    return value.map((item) => substituteExpressions(item, variables))
  }
  if (value && typeof value === 'object') {
    const out = {}
    for (const [key, item] of Object.entries(value)) {
      out[key] = substituteExpressions(item, variables)
    }
    return out
  }
  return value
}

function makeContext(id, cache) {
  const variableValues = new Map()
  cache.variableValues = variableValues

  async function evaluateFeedback(controlId) {
    const instance = cache.feedbackInstances.get(controlId)
    if (!instance) return null
    const definition = cache.feedbackDefinitions?.[instance.feedbackId]
    if (!definition?.callback) return null
    try {
      const style = await definition.callback(
        { id: controlId, controlId, options: substituteExpressions(instance.options, cache.variableValues) },
        {},
      )
      // Modules often return text containing `$(var)`; the host resolves it.
      const resolved = style
        ? substituteExpressions(style, cache.variableValues)
        : style
      console.error(`[ideck] feedback ${instance.feedbackId} style=${JSON.stringify(resolved)}`)
      if (resolved) {
        emit({
          event: 'feedback.updated',
          connectionId: id,
          values: [{ id: controlId, style: resolved }],
        })
      }
      return resolved ?? null
    } catch (error) {
      console.error(`[ideck] feedback ${instance.feedbackId} failed: ${error}`)
      log('debug', `feedback ${instance.feedbackId} failed: ${error}`)
      return null
    }
  }

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

    setPresetDefinitions(structure, definitions) {
      // Companion calls this with (structure, definitions). The definitions are
      // what actually hold `steps`, so both shapes are accepted and the ids are
      // kept for execution.
      const fromDefinitions = definitions ?? (Array.isArray(structure) ? null : structure)
      if (fromDefinitions) {
        cache.presets = Object.entries(fromDefinitions).map(([presetId, preset]) => ({
          presetId,
          ...preset,
        }))
      } else {
        cache.presets = Object.entries(structure ?? {}).map(([presetId, preset]) => ({
          presetId,
          ...preset,
        }))
      }
      cache.presetStructure = structure ?? null
      emit({ event: 'definitions.updated', connectionId: id, kind: 'presets', definitions: cache.presets })
    },

    updateFeedbackValues(values) {
      const updates = Object.entries(values ?? {}).map(([feedbackId, value]) => ({
        id: `${id}:${feedbackId}`,
        style: value?.style ?? value ?? {},
      }))
      if (updates.length) {
        emit({ event: 'feedback.updated', connectionId: id, values: updates })
      }
    },

    async checkFeedbacks(types) {
      const wanted = Array.isArray(types) && types.length ? new Set(types) : null
      const values = []
      for (const [feedbackId, definition] of Object.entries(cache.feedbackDefinitions ?? {})) {
        if (wanted && !wanted.has(feedbackId)) continue
        if (!definition?.callback) continue
        try {
          const style = await definition.callback(
            {
              id: `${id}:${feedbackId}`,
              controlId: `${id}:${feedbackId}`,
              options: {},
            },
            {},
          )
          if (style) values.push({ id: `${id}:${feedbackId}`, style })
        } catch {
          // A feedback that needs a live device/options is skipped until bound.
        }
      }
      if (values.length) {
        emit({ event: 'feedback.updated', connectionId: id, values })
      }
    },

    checkFeedbacksById(ids) {
      return this.checkFeedbacks(ids)
    },

    checkFeedbacksByTypes(types) {
      return this.checkFeedbacks(types)
    },
    subscribeActions() {},
    unsubscribeActions() {},
    subscribeFeedbacks() {},
    unsubscribeFeedbacks() {},

    async setFeedback(params) {
      const controlId = params.controlId ?? `${id}:${params.feedbackId}`
      cache.feedbackInstances.set(controlId, {
        feedbackId: params.feedbackId,
        // Keep expressions raw: they are resolved when the feedback is evaluated.
        options: params.options ?? {},
      })
      return evaluateFeedback(controlId)
    },

    removeFeedback(controlId) {
      cache.feedbackInstances.delete(controlId)
    },

    async evaluateFeedbacks() {
      for (const controlId of cache.feedbackInstances.keys()) {
        await evaluateFeedback(controlId)
      }
    },

    async evaluateFeedback(controlId) {
      return evaluateFeedback(controlId)
    },

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
    feedbackInstances: new Map(),
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
      options: substituteExpressions(params.options ?? {}, connection.cache.variableValues),
      controlId: `${params.connectionId}:${params.actionId}`,
    },
    {},
  )
  return { executed: true, result: result ?? null }
}

// Presets arrive either as a flat map (id -> preset) or as sections that carry a
// nested `definitions` list, so both shapes are searched.
function findPreset(entries, presetId) {
  for (const entry of entries ?? []) {
    if (entry.presetId === presetId || (entry.id === presetId && entry.steps)) {
      return entry
    }
    if (Array.isArray(entry.definitions)) {
      const nested = entry.definitions.find((item) => item.id === presetId)
      if (nested) return nested
    }
  }
  return null
}

// Runs the actions a preset defines for one phase (down/up/rotate_left/rotate_right).
// Companion binds dials and keys to presets, so this is what a dial does when it
// is bound to one.
async function runPresetStep(params) {
  const connection = connections.get(params.connectionId)
  if (!connection) throw new Error(`unknown connection ${params.connectionId}`)
  const preset = findPreset(connection.cache.presets ?? [], params.presetId)
  if (!preset) throw new Error(`unknown preset ${params.presetId}`)
  const step = (preset.steps ?? [])[params.step ?? 0] ?? {}
  const actions = step[params.phase] ?? []
  let executed = 0
  for (const item of actions) {
    const definition = connection.cache.actionDefinitions?.[item.actionId]
    if (!definition?.callback) continue
    if (item.delay) await new Promise((resolve) => setTimeout(resolve, item.delay))
    await definition.callback(
      {
        id: `preset-${params.presetId}-${executed}`,
        actionId: item.actionId,
        options: substituteExpressions(item.options ?? {}, connection.cache.variableValues),
        controlId: `${params.connectionId}:${params.presetId}`,
      },
      {},
    )
    executed += 1
  }
  return { executed }
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
    case 'connection.runPresetStep':
      return runPresetStep(params)
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
    case 'connection.setFeedback': {
      const connection = connections.get(params.connectionId)
      if (!connection) throw new Error(`unknown connection ${params.connectionId}`)
      const style = await connection.context.setFeedback(params)
      return { connectionId: params.connectionId, style: style ?? null }
    }
    case 'connection.removeFeedback': {
      const connection = connections.get(params.connectionId)
      if (!connection) throw new Error(`unknown connection ${params.connectionId}`)
      connection.context.removeFeedback(params.controlId)
      return { connectionId: params.connectionId }
    }
    case 'connection.evaluateFeedbacks': {
      const connection = connections.get(params.connectionId)
      if (!connection) throw new Error(`unknown connection ${params.connectionId}`)
      await connection.context.evaluateFeedbacks()
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
