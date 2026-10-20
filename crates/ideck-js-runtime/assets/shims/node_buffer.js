// `node:buffer` shim: exposes the runtime Buffer global as a module export, since
// libraries write `class X extends require('buffer').Buffer`.

const BufferImpl = globalThis.Buffer;

export default { Buffer: BufferImpl, constants: { MAX_LENGTH: 0x7fffffff } };
export const Buffer = BufferImpl;
export const constants = { MAX_LENGTH: 0x7fffffff };