// Minimal `Buffer` global for the embedded JS runtime.
//
// Many plugin dependencies do `Buffer.from(value, 'base64')` while loading, so a
// missing Buffer stops them before they can run. Boa cannot subclass Uint8Array,
// so a Buffer is a Uint8Array with the common node methods attached.

(function (global) {
  if (typeof global.Buffer !== "undefined") return;

  var B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

  function encodeUtf8(text) {
    var out = [];
    for (var i = 0; i < text.length; i++) {
      var code = text.charCodeAt(i);
      if (code < 0x80) {
        out.push(code);
      } else if (code < 0x800) {
        out.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
      } else {
        out.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 0x3f), 0x80 | (code & 0x3f));
      }
    }
    return new Uint8Array(out);
  }

  function decodeUtf8(bytes) {
    var out = "";
    for (var i = 0; i < bytes.length; i++) {
      var b = bytes[i];
      if (b < 0x80) {
        out += String.fromCharCode(b);
      } else if (b < 0xe0) {
        out += String.fromCharCode(((b & 0x1f) << 6) | (bytes[++i] & 0x3f));
      } else {
        out += String.fromCharCode(
          ((b & 0x0f) << 12) | ((bytes[++i] & 0x3f) << 6) | (bytes[++i] & 0x3f),
        );
      }
    }
    return out;
  }

  function encodeBase64(bytes) {
    var out = "";
    for (var i = 0; i < bytes.length; i += 3) {
      var b0 = bytes[i];
      var b1 = i + 1 < bytes.length ? bytes[i + 1] : 0;
      var b2 = i + 2 < bytes.length ? bytes[i + 2] : 0;
      out += B64[b0 >> 2];
      out += B64[((b0 & 3) << 4) | (b1 >> 4)];
      out += i + 1 < bytes.length ? B64[((b1 & 15) << 2) | (b2 >> 6)] : "=";
      out += i + 2 < bytes.length ? B64[b2 & 63] : "=";
    }
    return out;
  }

  function decodeBase64(text) {
    var clean = String(text).replace(/[^A-Za-z0-9+/=]/g, "");
    var out = [];
    for (var i = 0; i < clean.length; i += 4) {
      var c0 = B64.indexOf(clean[i]);
      var c1 = B64.indexOf(clean[i + 1]);
      var c2 = clean[i + 2] === "=" ? -1 : B64.indexOf(clean[i + 2]);
      var c3 = clean[i + 3] === "=" ? -1 : B64.indexOf(clean[i + 3]);
      out.push(((c0 << 2) | (c1 >> 4)) & 0xff);
      if (c2 >= 0) out.push(((c1 << 4) | (c2 >> 2)) & 0xff);
      if (c3 >= 0) out.push(((c2 << 6) | c3) & 0xff);
    }
    return new Uint8Array(out);
  }

  function encodeHex(bytes) {
    var out = "";
    for (var i = 0; i < bytes.length; i++) {
      out += (bytes[i] < 16 ? "0" : "") + bytes[i].toString(16);
    }
    return out;
  }

  function decodeHex(text) {
    var clean = String(text).replace(/[^0-9a-fA-F]/g, "");
    var out = [];
    for (var i = 0; i + 1 < clean.length; i += 2) {
      out.push(parseInt(clean.substr(i, 2), 16));
    }
    return new Uint8Array(out);
  }

  function toBytes(value, encoding) {
    if (value instanceof Uint8Array) return new Uint8Array(value);
    if (typeof value === "string") {
      var enc = String(encoding || "utf8").toLowerCase();
      if (enc === "hex") return decodeHex(value);
      if (enc === "base64") return decodeBase64(value);
      return encodeUtf8(value);
    }
    if (Array.isArray(value)) return new Uint8Array(value);
    if (value && typeof value.length === "number") return new Uint8Array(value);
    return new Uint8Array(0);
  }

  function decorate(bytes) {
    var buf = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    Object.defineProperty(buf, "__isBuffer", { value: true });
    buf.toString = function (encoding) {
      var enc = String(encoding || "utf8").toLowerCase();
      if (enc === "hex") return encodeHex(this);
      if (enc === "base64") return encodeBase64(this);
      if (enc === "latin1" || enc === "binary") {
        var out = "";
        for (var i = 0; i < this.length; i++) out += String.fromCharCode(this[i]);
        return out;
      }
      return decodeUtf8(this);
    };
    buf.toJSON = function () {
      return { type: "Buffer", data: Array.prototype.slice.call(this) };
    };
    buf.slice = function (start, end) {
      return decorate(Uint8Array.prototype.slice.call(this, start, end));
    };
    buf.subarray = function (start, end) {
      return decorate(Uint8Array.prototype.subarray.call(this, start, end));
    };
    buf.equals = function (other) {
      if (!other || other.length !== this.length) return false;
      for (var i = 0; i < this.length; i++) if (this[i] !== other[i]) return false;
      return true;
    };
    buf.copy = function (target, targetStart) {
      var offset = targetStart || 0;
      var written = Math.min(this.length, target.length - offset);
      for (var i = 0; i < written; i++) target[offset + i] = this[i];
      return written;
    };
    buf.write = function (text, offset, length, encoding) {
      var source = toBytes(text, encoding);
      var start = offset || 0;
      var written = Math.min(source.length, this.length - start);
      for (var i = 0; i < written; i++) this[start + i] = source[i];
      return written;
    };
    return buf;
  }

  var BufferImpl = {
    from: function (value, encoding) {
      return decorate(toBytes(value, encoding));
    },
    alloc: function (size, fill) {
      var buf = decorate(new Uint8Array(size));
      if (fill !== undefined) buf.fill(fill);
      return buf;
    },
    allocUnsafe: function (size) {
      return decorate(new Uint8Array(size));
    },
    concat: function (list) {
      var total = 0;
      for (var i = 0; i < list.length; i++) total += list[i].length;
      var out = new Uint8Array(total);
      var offset = 0;
      for (var j = 0; j < list.length; j++) {
        out.set(list[j], offset);
        offset += list[j].length;
      }
      return decorate(out);
    },
    isBuffer: function (value) {
      return Boolean(value && value.__isBuffer);
    },
    byteLength: function (value, encoding) {
      return toBytes(value, encoding).length;
    },
  };

  global.Buffer = BufferImpl;
})(typeof globalThis !== "undefined" ? globalThis : this);
