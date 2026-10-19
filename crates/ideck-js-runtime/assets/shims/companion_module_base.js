// Minimal `@companion-module/base` compatibility shim for embedded Boa.
// Covers the surface used by most Companion 3 modules.

export class InstanceBase {
  constructor(context) {
    this.context = context
    if (context && typeof context === 'object') {
      Object.assign(this, context)
    }
  }

  async init(_config, _isFirstInit, _secrets) {}
  async destroy() {}
  async configUpdated(_config, _secrets) {}
  async updateConfig(_config, _secrets) {}

  getConfigFields() {
    return []
  }

  updateStatus(status, message) {
    this.context?.updateStatus?.(status, message)
  }

  setActionDefinitions(actions) {
    this.context?.setActionDefinitions?.(actions)
  }

  subscribeActions(ids) {
    this.context?.subscribeActions?.(ids)
  }

  unsubscribeActions(ids) {
    this.context?.unsubscribeActions?.(ids)
  }

  setFeedbackDefinitions(feedbacks) {
    this.context?.setFeedbackDefinitions?.(feedbacks)
  }

  checkFeedbacks(...types) {
    this.context?.checkFeedbacks?.(...types)
  }

  setVariableDefinitions(variables) {
    this.context?.setVariableDefinitions?.(variables)
  }

  setVariableValues(values) {
    this.context?.setVariableValues?.(values)
  }

  getVariableValue(id) {
    return this.context?.getVariableValue?.(id)
  }

  setPresetDefinitions(presets) {
    this.context?.setPresetDefinitions?.(presets)
  }

  log(level, message) {
    globalThis.console?.log?.(`[${level}] ${message}`)
  }

  saveConfig(config, secrets) {
    this.context?.saveConfig?.(config, secrets)
  }
}

export function combineRgb(r, g, b) {
  return ((r & 0xff) << 16) | ((g & 0xff) << 8) | (b & 0xff)
}

export function combineRgbRev(rgb) {
  return { r: (rgb >> 16) & 0xff, g: (rgb >> 8) & 0xff, b: rgb & 0xff }
}

export const RGB = { Black: 0x000000, White: 0xffffff, Red: 0xff0000, Green: 0x00ff00, Blue: 0x0000ff }

export default { InstanceBase, combineRgb, combineRgbRev, RGB }
