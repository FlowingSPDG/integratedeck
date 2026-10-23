export function formatUserError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object") {
    const o = err as { message?: string; data?: string };
    if (typeof o.message === "string" && o.message.length > 0) return o.message;
    if (typeof o.data === "string" && o.data.length > 0) return o.data;
  }
  return String(err);
}
