import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";

/**
 * Native settings editor for Companion / plugin actions without a usable
 * Property Inspector. Keeps the persisted settings as JSON, but exposes a
 * structured key/value form for flat objects and a validated, syntax
 * highlighted raw JSON mode ("advanced JSON").
 *
 * Validation contract: `onSave` is only called with text that parses as JSON.
 */

type JsonRecord = Record<string, unknown>;

interface ParseSuccess {
  ok: true;
  value: unknown;
}

interface ParseFailure {
  ok: false;
  error: string;
}

function parseJson(text: string): ParseSuccess | ParseFailure {
  try {
    return { ok: true, value: JSON.parse(text) };
  } catch (err) {
    return { ok: false, error: err instanceof Error ? err.message : String(err) };
  }
}

function prettyJson(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

function isPlainObject(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFlatObject(value: unknown): value is JsonRecord {
  return (
    isPlainObject(value) &&
    Object.values(value).every(
      (v) => v === null || ["string", "number", "boolean"].includes(typeof v),
    )
  );
}

const JSON_TOKEN =
  /("(?:\\.|[^"\\])*")(\s*:)?|\b(true|false|null)\b|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g;

function highlightJson(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let last = 0;
  let match: RegExpExecArray | null;
  let key = 0;

  JSON_TOKEN.lastIndex = 0;
  while ((match = JSON_TOKEN.exec(text)) !== null) {
    if (match.index > last) nodes.push(text.slice(last, match.index));
    const [token, str, colon, literal, number] = match;
    if (str !== undefined) {
      nodes.push(
        <span key={key++} className={colon ? "json-key" : "json-string"}>
          {str}
        </span>,
      );
      if (colon) nodes.push(colon);
    } else if (literal !== undefined) {
      nodes.push(
        <span key={key++} className={literal === "null" ? "json-null" : "json-boolean"}>
          {literal}
        </span>,
      );
    } else if (number !== undefined) {
      nodes.push(
        <span key={key++} className="json-number">
          {number}
        </span>,
      );
    }
    last = match.index + token.length;
  }
  if (last < text.length) nodes.push(text.slice(last));
  return nodes;
}

function valueKind(value: unknown): "string" | "number" | "boolean" | "null" | "nested" {
  if (value === null) return "null";
  if (typeof value === "string") return "string";
  if (typeof value === "number") return "number";
  if (typeof value === "boolean") return "boolean";
  return "nested";
}

export function SettingsEditor({
  slotId,
  settingsJson,
  onSave,
}: {
  slotId: string | null;
  settingsJson: string;
  onSave: (json: string) => void;
}) {
  const [text, setText] = useState(() => prettyJson(parseOrEmpty(settingsJson)));
  const [advanced, setAdvanced] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const savedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const syncingRef = useRef(false);
  const lastSlotIdRef = useRef<string | null>(slotId);
  const textRef = useRef(text);
  const highlightRef = useRef<HTMLPreElement | null>(null);
  textRef.current = text;

  // Re-seed the editor when the selected slot or its persisted settings change
  // (Property Inspector update, slot switch, external refresh). A settings
  // round-trip caused by our own autosave is ignored so the caret stays put.
  useEffect(() => {
    const sameSlot = lastSlotIdRef.current === slotId;
    lastSlotIdRef.current = slotId;
    const current = parseJson(textRef.current);
    const incoming = parseJson(prettyJson(parseOrEmpty(settingsJson)));
    if (
      sameSlot &&
      current.ok &&
      incoming.ok &&
      JSON.stringify(current.value) === JSON.stringify(incoming.value)
    ) {
      return;
    }

    syncingRef.current = true;
    const seed = prettyJson(parseOrEmpty(settingsJson));
    setText(seed);
    setError(null);
    setSaved(false);
    setNewKey("");
    setNewValue("");
    setFormError(null);
    const id = window.setTimeout(() => {
      syncingRef.current = false;
    }, 0);
    return () => window.clearTimeout(id);
  }, [slotId, settingsJson]);

  useEffect(
    () => () => {
      if (savedTimer.current) clearTimeout(savedTimer.current);
    },
    [],
  );

  const parsed = useMemo(() => parseJson(text), [text]);
  const highlighted = useMemo(() => highlightJson(text), [text]);
  const flat = parsed.ok && isFlatObject(parsed.value) ? parsed.value : null;

  const markSaved = () => {
    setSaved(true);
    if (savedTimer.current) clearTimeout(savedTimer.current);
    savedTimer.current = setTimeout(() => setSaved(false), 1500);
  };

  const commit = (next: JsonRecord, options?: { text?: string }) => {
    const nextText = options?.text ?? prettyJson(next);
    setText(nextText);
    setError(null);
    setSaved(false);
    if (syncingRef.current) return;
    onSave(nextText);
    markSaved();
  };

  const updateValue = (key: string, value: unknown) => {
    if (!flat) return;
    commit({ ...flat, [key]: value });
  };

  const removeKey = (key: string) => {
    if (!flat) return;
    const next = { ...flat };
    delete next[key];
    commit(next);
  };

  const addKey = () => {
    const key = newKey.trim();
    if (!key) {
      setFormError("キー名を入力してください");
      return;
    }
    if (flat && Object.prototype.hasOwnProperty.call(flat, key)) {
      setFormError(`キー "${key}" は既に存在します`);
      return;
    }
    setFormError(null);
    let value: unknown = newValue;
    if (newValue === "true" || newValue === "false") value = newValue === "true";
    else if (newValue !== "" && !Number.isNaN(Number(newValue))) value = Number(newValue);
    commit({ ...(flat ?? {}), [key]: value });
    setNewKey("");
    setNewValue("");
  };

  const onTextChange = (next: string) => {
    setText(next);
    setSaved(false);
    const result = parseJson(next);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setError(null);
    if (!syncingRef.current) {
      onSave(next);
      markSaved();
    }
  };

  const showForm = !advanced && flat !== null;
  const showNestedNotice = !advanced && parsed.ok && !isFlatObject(parsed.value);

  return (
    <div className="settings-editor">
      <div className="settings-editor-toolbar">
        <label className="settings-mode-toggle">
          <input
            type="checkbox"
            checked={advanced}
            onChange={(e) => setAdvanced(e.target.checked)}
          />
          <span>詳細 JSON で編集</span>
        </label>
        <span className="settings-save-state" aria-live="polite">
          {error ? "未保存: 無効な JSON" : saved ? "保存しました" : "自動保存 (有効な JSON のみ)"}
        </span>
      </div>

      {error ? (
        <p className="settings-error" role="alert">
          JSON エラー: {error}
        </p>
      ) : null}

      {showNestedNotice ? (
        <p className="muted settings-nested-notice">
          ネストした設定のためフォーム表示できません。下の JSON を直接編集してください。
        </p>
      ) : null}

      {showForm ? (
        <div className="settings-form">
          {Object.keys(flat).length === 0 ? (
            <p className="muted">設定項目はありません。</p>
          ) : null}
          {Object.entries(flat).map(([key, value]) => {
            const kind = valueKind(value);
            return (
              <div className="settings-row" key={key}>
                <label className="settings-row-key" title={key}>
                  {key}
                </label>
                {kind === "boolean" ? (
                  <input
                    type="checkbox"
                    checked={Boolean(value)}
                    onChange={(e) => updateValue(key, e.target.checked)}
                  />
                ) : kind === "number" ? (
                  <input
                    type="number"
                    value={String(value)}
                    onChange={(e) => {
                      const n = Number(e.target.value);
                      if (!Number.isNaN(n)) updateValue(key, n);
                    }}
                  />
                ) : kind === "null" ? (
                  <input
                    type="text"
                    value="null"
                    readOnly
                    title="null は詳細 JSON で編集してください"
                  />
                ) : (
                  <input
                    type="text"
                    value={String(value)}
                    onChange={(e) => updateValue(key, e.target.value)}
                  />
                )}
                <button
                  type="button"
                  className="btn-xs settings-row-remove"
                  onClick={() => removeKey(key)}
                  title="この項目を削除"
                >
                  削除
                </button>
              </div>
            );
          })}
          <div className="settings-add-row">
            <input
              type="text"
              placeholder="キー"
              value={newKey}
              onChange={(e) => {
                setNewKey(e.target.value);
                setFormError(null);
              }}
            />
            <input
              type="text"
              placeholder="値"
              value={newValue}
              onChange={(e) => setNewValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") addKey();
              }}
            />
            <button type="button" className="btn-xs" onClick={addKey}>
              項目を追加
            </button>
          </div>
          {formError ? (
            <p className="settings-error" role="alert">
              {formError}
            </p>
          ) : null}
        </div>
      ) : null}

      {advanced || !parsed.ok || showNestedNotice ? (
        <div className={`json-editor${error ? " json-editor-invalid" : ""}`}>
          <pre className="json-highlight" aria-hidden="true" ref={highlightRef}>
            {highlighted}
            {"\n"}
          </pre>
          <textarea
            id="settings-json"
            rows={Math.min(14, Math.max(6, text.split("\n").length))}
            spellCheck={false}
            wrap="off"
            value={text}
            onChange={(e) => onTextChange(e.target.value)}
            onScroll={(e) => {
              const pre = highlightRef.current;
              if (!pre) return;
              pre.scrollTop = e.currentTarget.scrollTop;
              pre.scrollLeft = e.currentTarget.scrollLeft;
            }}
            aria-label="Settings JSON"
            aria-invalid={error ? true : undefined}
          />
        </div>
      ) : null}
    </div>
  );
}

function parseOrEmpty(text: string): unknown {
  const result = parseJson(text);
  return result.ok ? result.value : {};
}
