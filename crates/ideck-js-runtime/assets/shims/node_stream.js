// Minimal `node:stream` shim for the embedded JS runtime.
//
// Libraries frequently `class X extends stream.Readable`. The superclass must be
// a real constructor, so these are benign classes that buffer in memory instead
// of moving bytes anywhere.

// Self-contained EventEmitter: shim modules cannot import relative paths.
class EventEmitter {
  constructor() {
    this._listeners = new Map();
  }
  on(event, listener) {
    const key = String(event);
    if (!this._listeners.has(key)) this._listeners.set(key, []);
    this._listeners.get(key).push(listener);
    return this;
  }
  once(event, listener) {
    const wrapper = (...args) => {
      this.off(event, wrapper);
      listener(...args);
    };
    return this.on(event, wrapper);
  }
  off(event, listener) {
    const list = this._listeners.get(String(event));
    if (list) {
      const idx = list.indexOf(listener);
      if (idx >= 0) list.splice(idx, 1);
    }
    return this;
  }
  emit(event, ...args) {
    const list = [...(this._listeners.get(String(event)) ?? [])];
    for (const listener of list) listener(...args);
    return list.length > 0;
  }
  removeListener(event, listener) {
    return this.off(event, listener);
  }
  addListener(event, listener) {
    return this.on(event, listener);
  }
}

class Stream extends EventEmitter {
  pipe(destination) {
    return destination;
  }
  read() {
    return null;
  }
  write(chunk, _encoding, callback) {
    if (typeof _encoding === "function") _encoding(chunk);
    else if (typeof callback === "function") callback(chunk);
    return true;
  }
  end(callback) {
    if (typeof callback === "function") callback();
    this.emit("end");
    return this;
  }
  destroy(error) {
    if (error) this.emit("error", error);
    this.emit("close");
    return this;
  }
  pause() {
    return this;
  }
  resume() {
    return this;
  }
}

class Readable extends Stream {
  constructor(options) {
    super();
    this._options = options || {};
    this.readable = true;
    this._chunks = [];
  }
  push(chunk) {
    if (chunk === null) {
      this.readable = false;
      this.emit("end");
      return false;
    }
    this._chunks.push(chunk);
    this.emit("data", chunk);
    return true;
  }
  read() {
    return this._chunks.shift() ?? null;
  }
}

class Writable extends Stream {
  constructor(options) {
    super();
    this._options = options || {};
    this.writable = true;
  }
  _write() {}
}

class Duplex extends Readable {
  constructor(options) {
    super(options);
    this.writable = true;
  }
}

class Transform extends Duplex {}

const promises = {
  pipeline: () => Promise.resolve(),
  finished: () => Promise.resolve(),
};

export default Stream;
export {
  Stream,
  Readable,
  Writable,
  Duplex,
  Transform,
  promises,
};
export const pipeline = () => Promise.resolve();
export const finished = () => Promise.resolve();
