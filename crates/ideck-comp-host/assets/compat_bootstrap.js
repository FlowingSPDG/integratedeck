/**
 * integratedeck Companion compat bootstrap (Boa, no Node.js).
 * Host-side lifecycle; plugin modules load via Rust ESM resolver.
 */

const BANNED_PROPS = new Set([
  '__proto__',
  'constructor',
  'prototype',
  'hasOwnProperty',
  'toString',
  'valueOf',
])

function emit(event, data) {
  globalThis.__ideckEmit(event, JSON.stringify(data ?? {}))
}

function joinPath(...parts) {
  const cleaned = parts
    .flatMap((p) => String(p).replace(/\\/g, '/').split('/'))
    .filter((p, i) => p.length > 0 || i === 0)
  if (cleaned.length === 0) return '.'
  if (/^[A-Za-z]:$/.test(cleaned[0])) {
    return cleaned.join('/').replace(/\//g, '\\')
  }
  return cleaned.join('/')
}

function runUpgradeScripts(lastUpgradeIndex, scripts, config, secrets) {
  let updatedConfig = config
  let updatedSecrets = secrets
  const start = (lastUpgradeIndex ?? -1) + 1
  for (let i = start; i < scripts.length; i++) {
    const script = scripts[i]
    if (typeof script !== 'function') continue
    const result = script(updatedConfig, updatedSecrets)
    if (result && typeof result === 'object') {
      if ('updatedConfig' in result && result.updatedConfig !== undefined) {
        updatedConfig = result.updatedConfig
      }
      if ('updatedSecrets' in result && result.updatedSecrets !== undefined) {
        updatedSecrets = result.updatedSecrets
      }
    }
  }
  return { updatedConfig, updatedSecrets }
}

class ActionManager {
  constructor(setActionDefinitions) {
    this.setActionDefinitions = setActionDefinitions
    this.actionDefinitions = new Map()
  }

  setActionDefinitions(actions) {
    const hostActions = []
    this.actionDefinitions.clear()
    for (const [actionId, action] of Object.entries(actions ?? {})) {
      if (!action) continue
      if (BANNED_PROPS.has(actionId)) {
        throw new Error(`Action id "${actionId}" is a reserved word`)
      }
      hostActions.push({
        id: actionId,
        name: action.name,
        sortName: action.sortName,
        description: action.description,
        options: action.options,
        optionsToMonitorForSubscribe: action.optionsToMonitorForSubscribe,
        hasLearn: !!action.learn,
        learnTimeout: action.learnTimeout,
        hasLifecycleFunctions: !!(action.subscribe || action.unsubscribe),
      })
      this.actionDefinitions.set(actionId, action)
    }
    this.setActionDefinitions(hostActions)
  }

  subscribeActions() {}
  unsubscribeActions() {}

  async handleExecuteAction(action, surfaceId) {
    const definition = this.actionDefinitions.get(action.actionId)
    if (!definition?.callback) {
      return {
        success: false,
        errorMessage: `Action definition not found for: ${action.actionId}`,
      }
    }
    const context = {
      type: 'action',
      setCustomVariableValue: () => {
        throw new Error('setCustomVariableValue is not available during execute')
      },
    }
    try {
      await definition.callback(
        {
          id: action.id,
          actionId: action.actionId,
          controlId: action.controlId,
          options: action.options,
          surfaceId,
        },
        context,
      )
      return { success: true }
    } catch (e) {
      return {
        success: false,
        errorMessage: e instanceof Error ? e.message : String(e),
      }
    }
  }
}

class FeedbackManager {
  constructor(setFeedbackDefinitions, updateFeedbackValues) {
    this.setFeedbackDefinitions = setFeedbackDefinitions
    this.updateFeedbackValues = updateFeedbackValues
  }

  setFeedbackDefinitions(feedbacks) {
    const hostFeedbacks = []
    for (const [feedbackId, feedback] of Object.entries(feedbacks ?? {})) {
      if (!feedback) continue
      if (BANNED_PROPS.has(feedbackId)) {
        throw new Error(`Feedback id "${feedbackId}" is a reserved word`)
      }
      hostFeedbacks.push({
        id: feedbackId,
        name: feedback.name,
        type: feedback.type,
        defaultStyle: feedback.defaultStyle,
        options: feedback.options,
        hasLifecycleFunctions: !!(feedback.subscribe || feedback.unsubscribe),
      })
    }
    this.setFeedbackDefinitions(hostFeedbacks)
  }

  unsubscribeFeedbacks() {}
  checkFeedbacks() {}
  checkFeedbacksById() {}
}

class ConnectionHost {
  constructor(connectionId, host, InstanceClass, upgradeScripts) {
    this.connectionId = connectionId
    this.host = host
    this.initialized = false
    this.lastConfig = {}
    this.lastSecrets = {}
    this.variableDefinitions = new Map()
    this.variableValues = new Map()
    this.actionManager = new ActionManager((actions) => host.setActionDefinitions(actions))
    this.feedbackManager = new FeedbackManager(
      (feedbacks) => host.setFeedbackDefinitions(feedbacks),
      (values) => host.updateFeedbackValues(values),
    )

    this.instanceContext = {
      _isInstanceContext: true,
      id: connectionId,
      label: connectionId,
      upgradeScripts: upgradeScripts ?? [],

      saveConfig: (newConfig, newSecrets) => {
        if (newConfig !== undefined) this.lastConfig = newConfig
        if (newSecrets !== undefined) this.lastSecrets = newSecrets
        host.saveConfig(newConfig, newSecrets)
      },
      updateStatus: (status, message) => host.setStatus(status, message),
      oscSend: (h, port, path, args) => host.sendOSC(h, port, path, args),
      recordAction: () => {
        throw new Error('Action recording is not supported')
      },

      setActionDefinitions: (actions) => this.actionManager.setActionDefinitions(actions),
      subscribeActions: (actionIds) => this.actionManager.subscribeActions(actionIds),
      unsubscribeActions: (actionIds) => this.actionManager.unsubscribeActions(actionIds),

      setFeedbackDefinitions: (feedbacks) =>
        this.feedbackManager.setFeedbackDefinitions(feedbacks),
      unsubscribeFeedbacks: (feedbackIds) =>
        this.feedbackManager.unsubscribeFeedbacks(feedbackIds),
      checkFeedbacks: (feedbackTypes) => this.feedbackManager.checkFeedbacks(feedbackTypes),
      checkAllFeedbacks: () => this.feedbackManager.checkFeedbacks(null),
      checkFeedbacksById: (feedbackIds) =>
        this.feedbackManager.checkFeedbacksById(feedbackIds),

      setPresetDefinitions: () => {},
      setCompositeElementDefinitions: () => {},

      setVariableDefinitions: (variables) => {
        const hostVariables = []
        const hostValues = []
        this.variableDefinitions.clear()
        for (const [variableId, definition] of Object.entries(variables ?? {})) {
          if (BANNED_PROPS.has(variableId)) {
            throw new Error(`Variable id "${variableId}" is a reserved word`)
          }
          hostVariables.push({ id: variableId, name: definition.name })
          this.variableDefinitions.set(variableId, definition)
          if (!this.variableValues.has(variableId)) {
            this.variableValues.set(variableId, '')
            hostValues.push({ id: variableId, value: '' })
          }
        }
        host.setVariableDefinitions(hostVariables, hostValues)
      },

      setVariableValues: (values) => {
        const hostValues = []
        for (const [variableId, value] of Object.entries(values ?? {})) {
          if (BANNED_PROPS.has(variableId)) continue
          if (this.variableDefinitions.has(variableId)) {
            this.variableValues.set(variableId, value ?? '')
            hostValues.push({ id: variableId, value: value ?? '' })
          } else {
            hostValues.push({ id: variableId, value: undefined })
          }
        }
        host.setVariableValues(hostValues)
      },

      getVariableValue: (variableId) => this.variableValues.get(variableId),

      sharedUdpSocketHandlers: new Map(),
      sharedUdpSocketJoin: async (msg) => host.sharedUdpSocketJoin(msg),
      sharedUdpSocketLeave: async (msg) => host.sharedUdpSocketLeave(msg),
      sharedUdpSocketSend: async (msg) => host.sharedUdpSocketSend(msg),
    }

    this.instance = new InstanceClass(this.instanceContext)
  }

  async init({ label, isFirstInit, config, secrets, lastUpgradeIndex }) {
    if (this.initialized) throw new Error('Already initialized')
    this.lastConfig = config ?? {}
    this.lastSecrets = secrets ?? {}
    this.instanceContext.label = label

    if (isFirstInit) {
      const newConfig = {}
      const newSecrets = {}
      const fields = this.instance.getConfigFields?.() ?? []
      for (const field of fields) {
        if (!field || !('default' in field)) continue
        if (typeof field.type === 'string' && field.type.startsWith('secret')) {
          newSecrets[field.id] = field.default
        } else {
          newConfig[field.id] = field.default
        }
      }
      this.lastConfig = newConfig
      this.lastSecrets = newSecrets
      this.host.saveConfig(this.lastConfig, this.lastSecrets)
      lastUpgradeIndex = this.instanceContext.upgradeScripts.length - 1
    }

    const upgraded = runUpgradeScripts(
      lastUpgradeIndex,
      this.instanceContext.upgradeScripts,
      this.lastConfig,
      this.lastSecrets,
    )
    this.lastConfig = upgraded.updatedConfig
    this.lastSecrets = upgraded.updatedSecrets

    await this.instance.init(this.lastConfig, !!isFirstInit, this.lastSecrets)
    this.initialized = true
  }

  async destroy() {
    if (!this.initialized) return
    if (typeof this.instance.destroy === 'function') {
      await this.instance.destroy()
    }
    this.initialized = false
  }

  async executeAction(action, surfaceId) {
    return this.actionManager.handleExecuteAction(action, surfaceId)
  }
}

const connections = new Map()
const definitionsCache = new Map()

function cacheDefinitions(connectionId, patch) {
  const entry = definitionsCache.get(connectionId) ?? {
    actions: [],
    feedbacks: [],
    variables: [],
  }
  if (patch.actions) entry.actions = patch.actions
  if (patch.feedbacks) entry.feedbacks = patch.feedbacks
  if (patch.variables) entry.variables = patch.variables
  definitionsCache.set(connectionId, entry)
}

function createRustHost(connectionId, onConfigSave) {
  return {
    setStatus: (status, message) => {
      emit('connection.status', {
        connectionId,
        status,
        message: message ?? undefined,
      })
    },
    setActionDefinitions: (definitions) => {
      cacheDefinitions(connectionId, { actions: definitions })
      emit('definitions.updated', { connectionId, kind: 'actions', definitions })
    },
    setFeedbackDefinitions: (definitions) => {
      cacheDefinitions(connectionId, { feedbacks: definitions })
      emit('definitions.updated', { connectionId, kind: 'feedbacks', definitions })
    },
    setVariableDefinitions: (definitions, values) => {
      cacheDefinitions(connectionId, { variables: definitions })
      emit('definitions.updated', { connectionId, kind: 'variables', definitions, values })
    },
    setVariableValues: (values) => emit('variable.updated', { connectionId, values }),
    updateFeedbackValues: (values) => emit('feedback.updated', { connectionId, values }),
    saveConfig: (config, secrets) => {
      onConfigSave(config, secrets)
      emit('connection.config', { connectionId, config, secrets })
    },
    sendOSC: () => {},
    sharedUdpSocketJoin: async () => '',
    sharedUdpSocketLeave: async () => {},
    sharedUdpSocketSend: async () => {},
  }
}

async function loadModuleEntry(modulePath) {
  const entry = joinPath(modulePath, 'dist', 'index.js')
  const alt = joinPath(modulePath, 'index.js')
  const file = globalThis.__ideckFs.existsSync(entry)
    ? entry
    : globalThis.__ideckFs.existsSync(alt)
      ? alt
      : null
  if (!file) {
    throw new Error(`module entry not found: ${modulePath}`)
  }
  return import(globalThis.__ideckFs.toFileUrl(file))
}

async function initConnection(id, moduleId, label, config, secrets, modulesRoot) {
  const modulePath = joinPath(modulesRoot, moduleId)
  if (!globalThis.__ideckFs.existsSync(modulePath)) {
    throw new Error(`module directory not found: ${modulePath}`)
  }

  definitionsCache.set(id, { actions: [], feedbacks: [], variables: [] })

  const mod = await loadModuleEntry(modulePath)
  const InstanceClass = mod.default
  const upgradeScripts = mod.upgradeScripts ?? []

  if (typeof InstanceClass !== 'function') {
    throw new Error(`module ${moduleId} has no default export class`)
  }

  let savedConfig = config ?? {}
  let savedSecrets = secrets ?? {}

  const host = createRustHost(id, (newConfig, newSecrets) => {
    if (newConfig !== undefined) savedConfig = newConfig
    if (newSecrets !== undefined) savedSecrets = newSecrets
    const conn = connections.get(id)
    if (conn) {
      conn.config = savedConfig
      conn.secrets = savedSecrets
    }
  })

  const connectionHost = new ConnectionHost(id, host, InstanceClass, upgradeScripts)
  await connectionHost.init({
    label: label || moduleId,
    isFirstInit:
      !config || (typeof config === 'object' && Object.keys(config).length === 0),
    config: savedConfig,
    secrets: savedSecrets,
    lastUpgradeIndex: -1,
  })

  connections.set(id, {
    moduleId,
    modulePath,
    label: label || moduleId,
    config: savedConfig,
    secrets: savedSecrets,
    host: connectionHost,
  })

  emit('connection.status', { connectionId: id, status: 'ok', moduleId })
}

globalThis.__ideckHandle = async function __ideckHandle(method, params, modulesRoot) {
  switch (method) {
    case 'ping':
      return {
        pong: true,
        engine: 'ideck-comp-host-boa',
        connections: connections.size,
        modulesRoot,
      }

    case 'connection.add': {
      const { id, moduleId, config, label, secrets } = params
      await initConnection(id, moduleId, label ?? moduleId, config, secrets, modulesRoot)
      return { id }
    }

    case 'connection.remove': {
      const { connectionId } = params
      const conn = connections.get(connectionId)
      if (conn?.host) {
        await conn.host.destroy()
      }
      connections.delete(connectionId)
      definitionsCache.delete(connectionId)
      emit('connection.status', { connectionId, status: 'disconnected' })
      return { connectionId }
    }

    case 'connection.executeAction': {
      const { connectionId, actionId, options } = params
      const conn = connections.get(connectionId)
      if (!conn?.host) {
        throw new Error(`unknown connection ${connectionId}`)
      }
      const result = await conn.host.executeAction(
        {
          id: `action-${actionId}`,
          actionId,
          options: options ?? {},
          controlId: `${connectionId}:${actionId}`,
        },
        undefined,
      )
      if (!result.success) {
        throw new Error(result.errorMessage ?? 'executeAction failed')
      }
      return { executed: true, connectionId, actionId }
    }

    case 'connection.getDefinitions': {
      const { connectionId } = params
      const cached = definitionsCache.get(connectionId)
      return cached ?? { actions: [], feedbacks: [], variables: [] }
    }

    default:
      throw new Error(`unknown method: ${method}`)
  }
}
