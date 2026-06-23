export class EventEmitter {
  constructor() {
    this._listeners = new Map()
  }

  on(event, listener) {
    const key = String(event)
    if (!this._listeners.has(key)) this._listeners.set(key, [])
    this._listeners.get(key).push(listener)
    return this
  }

  once(event, listener) {
    const wrapper = (...args) => {
      this.off(event, wrapper)
      listener(...args)
    }
    return this.on(event, wrapper)
  }

  off(event, listener) {
    const key = String(event)
    const list = this._listeners.get(key)
    if (!list) return this
    const idx = list.indexOf(listener)
    if (idx >= 0) list.splice(idx, 1)
    return this
  }

  emit(event, ...args) {
    const key = String(event)
    const list = [...(this._listeners.get(key) ?? [])]
    for (const listener of list) {
      listener(...args)
    }
    return list.length > 0
  }

  removeListener(event, listener) {
    return this.off(event, listener)
  }

  addListener(event, listener) {
    return this.on(event, listener)
  }
}

export default EventEmitter
