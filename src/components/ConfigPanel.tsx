import { memo, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useDeck } from "../deck/DeckContext";
import {
  BUILTIN_OPEN_FOLDER,
  bindingStepLabel,
  folderChildPageId,
  normalizeBinding,
} from "../lib/binding";
import type { Profile, Slot } from "../lib/types";
import { CompanionOptionsEditor } from "./CompanionOptionsEditor";
import { SettingsEditor } from "./SettingsEditor";

function imagePreviewUrl(image?: { format: string; data: string }): string | null {
  if (!image?.data) return null;
  const fmt = image.format === "jpeg" ? "jpeg" : "png";
  return `data:image/${fmt};base64,${image.data}`;
}

export const ConfigPanel = memo(function ConfigPanel() {
  const {
    profile,
    selectedSlot,
    selectedSlotId,
    registerPiFrameRef,
    saveSlotAppearance,
    saveMultiAction,
    saveNativeSettings,
    setSwitchPageTarget,
    setActivePage,
    piNativeSettingsVisible,
    piSettingsJson,
    refresh,
  } = useDeck();

  const hasSlot = Boolean(selectedSlotId && selectedSlot);
  const binding = useMemo(
    () => normalizeBinding(selectedSlot?.binding),
    [selectedSlot?.binding],
  );

  const showMulti = hasSlot && binding?.type === "multi_action";
  const showBuiltin = hasSlot && binding?.type === "builtin";
  const showPi =
    hasSlot &&
    binding != null &&
    binding.type !== "multi_action" &&
    binding.type !== "builtin";

  return (
    <div className="config-panel">
      <p className={`config-placeholder${hasSlot ? " hidden" : ""}`}>
        キーを選択してアクションを設定してください
      </p>

      <SlotAppearanceFields
        hidden={!hasSlot}
        slotId={selectedSlotId}
        slot={selectedSlot}
        onSave={saveSlotAppearance}
      />

      <div className={`multi-action-editor${showMulti ? "" : " hidden"}`}>
        {showMulti && binding ? (
          <MultiActionEditor binding={binding} onSave={saveMultiAction} />
        ) : null}
      </div>

      <div className={`builtin-settings${showBuiltin ? "" : " hidden"}`}>
        {showBuiltin && binding && profile ? (
          <BuiltinSettings
            binding={binding}
            profile={profile}
            onSwitchPage={setSwitchPageTarget}
            onOpenFolder={(pageId) => void setActivePage(pageId)}
          />
        ) : null}
      </div>

      <PropertyInspectorSection
        hidden={!showPi}
        binding={showPi ? binding : undefined}
        slotId={selectedSlotId}
        registerPiFrameRef={registerPiFrameRef}
        onSettingsChange={saveNativeSettings}
        nativeSettingsVisible={piNativeSettingsVisible}
        settingsJson={piSettingsJson}
        profile={profile}
        onRefresh={refresh}
      />
    </div>
  );
});

function SlotAppearanceFields({
  hidden,
  slotId,
  slot,
  onSave,
}: {
  hidden: boolean;
  slotId: string | null;
  slot: Slot | undefined;
  onSave: (title?: string, imageBase64?: string, clearImage?: boolean) => Promise<void>;
}) {
  const [title, setTitle] = useState("");
  const [imagePreview, setImagePreview] = useState<string | null>(null);
  const imageInputRef = useRef<HTMLInputElement>(null);
  const syncingSlotIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (!slotId || !slot) return;
    syncingSlotIdRef.current = slotId;
    setTitle(slot.appearance?.title ?? slot.label ?? "");
    setImagePreview(imagePreviewUrl(slot.appearance?.default_image));
    if (imageInputRef.current) imageInputRef.current.value = "";
  }, [
    slotId,
    slot?.appearance?.title,
    slot?.label,
    slot?.appearance?.default_image?.data,
    slot?.appearance?.default_image?.format,
  ]);

  const saveTitle = () => {
    if (!slotId || syncingSlotIdRef.current !== slotId) return;
    void onSave(title);
  };

  return (
    <div className={`slot-appearance${hidden ? " hidden" : ""}`}>
      <h2>キーの表示</h2>
      <div className="appearance-fields">
        <label className="field-inline">
          タイトル
          <input
            type="text"
            placeholder="ボタンタイトル"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onBlur={saveTitle}
          />
        </label>
        <label className="field-inline">
          デフォルト画像
          <input
            ref={imageInputRef}
            type="file"
            accept="image/png,image/jpeg,image/webp"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (!file || !slotId) return;
              const reader = new FileReader();
              reader.onload = () => {
                const dataUrl = String(reader.result ?? "");
                void onSave(undefined, dataUrl).then(() => setImagePreview(dataUrl));
              };
              reader.readAsDataURL(file);
            }}
          />
        </label>
        <button
          type="button"
          className="btn-xs"
          onClick={() => {
            if (!slotId) return;
            if (imageInputRef.current) imageInputRef.current.value = "";
            void onSave(undefined, undefined, true).then(() => setImagePreview(null));
          }}
        >
          画像をクリア
        </button>
        <div className="slot-image-preview">
          {imagePreview ? <img src={imagePreview} alt="preview" /> : null}
        </div>
      </div>
    </div>
  );
}

function MultiActionEditor({
  binding,
  onSave,
}: {
  binding: NonNullable<ReturnType<typeof normalizeBinding>>;
  onSave: (steps: NonNullable<typeof binding.steps>, delayMs: number) => Promise<void>;
}) {
  const steps = binding.steps ?? [];
  const delayMs = binding.delayMs ?? 200;

  return (
    <>
      <h2>マルチアクション</h2>
      <p className="muted">アクションリストからステップをドラッグして追加</p>
      <ul className="multi-action-steps">
        {steps.length === 0 ? (
          <li className="muted">ステップなし — 右の Actions からドラッグ</li>
        ) : (
          steps.map((step, i) => (
            <li key={i} className="multi-action-step">
              <span>{bindingStepLabel(step)}</span>
              <label className="multi-action-delay">
                待機
                <input
                  type="number"
                  min={0}
                  max={60000}
                  defaultValue={step.delay_before_ms ?? 0}
                  onChange={(e) => {
                    const next = steps.map((entry, index) =>
                      index === i
                        ? { ...entry, delay_before_ms: Number(e.target.value) }
                        : entry,
                    );
                    void onSave(next, delayMs);
                  }}
                />
                ms
              </label>
              <button
                type="button"
                className="btn-xs"
                disabled={i === 0}
                onClick={() => {
                  const next = [...steps];
                  [next[i - 1], next[i]] = [next[i], next[i - 1]];
                  void onSave(next, delayMs);
                }}
              >
                ↑
              </button>
              <button
                type="button"
                className="btn-xs"
                disabled={i === steps.length - 1}
                onClick={() => {
                  const next = [...steps];
                  [next[i + 1], next[i]] = [next[i], next[i + 1]];
                  void onSave(next, delayMs);
                }}
              >
                ↓
              </button>
              <button
                type="button"
                className="btn-xs btn-remove-step"
                onClick={() => {
                  const next = [...steps];
                  next.splice(i, 1);
                  void onSave(next, delayMs);
                }}
              >
                削除
              </button>
            </li>
          ))
        )}
      </ul>
      <label className="field-inline">
        ステップ間隔 (ms)
        <input
          type="number"
          min={0}
          max={10000}
          defaultValue={delayMs}
          onChange={(e) => void onSave(steps, Number(e.target.value))}
        />
      </label>
    </>
  );
}

function BuiltinSettings({
  binding,
  profile,
  onSwitchPage,
  onOpenFolder,
}: {
  binding: NonNullable<ReturnType<typeof normalizeBinding>>;
  profile: NonNullable<ReturnType<typeof useDeck>["profile"]>;
  onSwitchPage: (targetPageId: string) => Promise<void>;
  onOpenFolder: (pageId: string) => void;
}) {
  return (
    <>
      <h2>ナビゲーション設定</h2>
      <div id="builtin-settings-body">
        {binding.actionId === "com.elgato.streamdeck.profile.rotate" ? (
          <label>
            切り替え先ページ
            <select
              defaultValue={
                ((binding.settings as Record<string, unknown> | undefined)?.targetPageId ??
                  (binding.settings as Record<string, unknown> | undefined)?.target_page_id) as
                  | string
                  | undefined
              }
              onChange={(e) => void onSwitchPage(e.target.value)}
            >
              {profile.pages.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
        ) : binding.actionId === BUILTIN_OPEN_FOLDER ? (
          <>
            <p className="muted">
              フォルダ内のキーを編集するには、このボタンを選択した状態でグリッドがフォルダページに切り替わります。
            </p>
            {folderChildPageId(binding.settings) ? (
              <button
                type="button"
                className="btn-xs"
                onClick={() => onOpenFolder(folderChildPageId(binding.settings)!)}
              >
                フォルダを開く
              </button>
            ) : null}
          </>
        ) : (
          <p className="muted">{binding.actionId ?? ""} — 追加設定は不要です</p>
        )}
      </div>
    </>
  );
}

function PropertyInspectorSection({
  hidden,
  binding,
  slotId,
  registerPiFrameRef,
  onSettingsChange,
  nativeSettingsVisible,
  settingsJson,
  profile,
  onRefresh,
}: {
  hidden: boolean;
  binding: ReturnType<typeof normalizeBinding> | undefined;
  slotId: string | null;
  registerPiFrameRef: (el: HTMLIFrameElement | null) => void;
  onSettingsChange: (json: string) => void;
  nativeSettingsVisible: boolean;
  settingsJson: string;
  profile: Profile | null;
  onRefresh: (opts?: { surfaceId?: string | null }) => Promise<void>;
}) {
  const isCompanion = binding?.type === "companion";
  const [feedbacks, setFeedbacks] = useState<Array<{ id: string; name: string }>>([]);

  useEffect(() => {
    if (!isCompanion || binding?.type !== "companion") {
      setFeedbacks([]);
      return;
    }
    let cancelled = false;
    void invoke<Array<{ id: string; name: string }>>("list_companion_feedbacks", {
      connectionId: binding.connectionId,
    })
      .then((list) => {
        if (!cancelled) setFeedbacks(list);
      })
      .catch(() => {
        if (!cancelled) setFeedbacks([]);
      });
    return () => {
      cancelled = true;
    };
  }, [isCompanion, binding]);

  const currentFeedback =
    slotId && profile?.companion_feedbacks
      ? profile.companion_feedbacks[slotId]
      : undefined;

  return (
    <div className={hidden ? "hidden" : undefined}>
      <h2 id="pi-title">
        {isCompanion
          ? "Companion アクション設定"
          : binding?.actionUuid
            ? `Stream Deck: ${binding.actionUuid}`
            : "Property Inspector"}
      </h2>
      <p className="muted" id="pi-status">
        {binding?.actionId ?? binding?.actionUuid ?? ""}
      </p>
      {isCompanion && slotId ? (
        <div className="companion-options">
          <label>
            フィードバック
            <select
              value={currentFeedback?.feedback_id ?? ""}
              onChange={async (event) => {
                const value = event.target.value;
                if (value) {
                  await invoke("set_slot_companion_feedback", {
                    args: { slotId, feedbackId: value, options: {} },
                  });
                } else {
                  await invoke("clear_slot_companion_feedback", { slotId });
                }
                await onRefresh();
              }}
            >
              <option value="">なし</option>
              {feedbacks.map((feedback) => (
                <option key={feedback.id} value={feedback.id}>
                  {feedback.name}
                </option>
              ))}
            </select>
          </label>
          {currentFeedback ? (
            <>
              <CompanionOptionsEditor
                key={`${slotId}:${currentFeedback.feedback_id}`}
                connectionId={currentFeedback.connection_id}
                section="feedbacks"
                definitionId={currentFeedback.feedback_id}
                options={
                  (currentFeedback.options as Record<string, unknown> | undefined) ?? {}
                }
                onChange={(options) => {
                  void invoke("set_slot_companion_feedback", {
                    args: {
                      slotId,
                      feedbackId: currentFeedback.feedback_id,
                      options,
                    },
                  }).then(() => onRefresh());
                }}
              />
              <details className="companion-json">
                <summary>JSON で編集</summary>
                <SettingsEditor
                  key={`${slotId}:feedback:json`}
                  slotId={`${slotId}:feedback`}
                  settingsJson={JSON.stringify(currentFeedback.options ?? {}, null, 2)}
                  onSave={(json) => {
                    try {
                      const options = JSON.parse(json) as unknown;
                      void invoke("set_slot_companion_feedback", {
                        args: {
                          slotId,
                          feedbackId: currentFeedback.feedback_id,
                          options,
                        },
                      }).then(() => onRefresh());
                    } catch {
                      // SettingsEditor already reports JSON errors.
                    }
                  }}
                />
              </details>
            </>
          ) : null}
          {binding?.type === "companion" && binding.actionId ? (
            <CompanionOptionsEditor
              key={`${slotId}:action`}
              connectionId={binding.connectionId ?? ""}
              section="actions"
              definitionId={binding.actionId}
              options={(binding.options as Record<string, unknown> | undefined) ?? {}}
              onChange={(options) => {
                void invoke("update_companion_options", {
                  args: { slotId, options },
                }).then(() => onRefresh());
              }}
            />
          ) : null}
          <details className="companion-json">
            <summary>JSON で編集</summary>
            <SettingsEditor
              slotId={slotId}
              settingsJson={JSON.stringify(
                binding?.type === "companion" ? (binding.options ?? {}) : {},
                null,
                2,
              )}
              onSave={(json) => {
                try {
                  const options = JSON.parse(json) as unknown;
                  void invoke("update_companion_options", {
                    args: { slotId, options },
                  });
                } catch {
                  // SettingsEditor already reports JSON errors.
                }
              }}
            />
          </details>
        </div>
      ) : null}
      <iframe
        ref={registerPiFrameRef}
        id="pi-frame"
        title="Property Inspector"
        sandbox="allow-scripts allow-same-origin"
        className={isCompanion || nativeSettingsVisible ? "hidden" : undefined}
      />
      <div
        className={`native-settings${nativeSettingsVisible ? "" : " hidden"}`}
        id="native-settings"
      >
        {nativeSettingsVisible ? (
          <SettingsEditor
            slotId={slotId}
            settingsJson={settingsJson}
            onSave={onSettingsChange}
          />
        ) : null}
      </div>
    </div>
  );
}
