import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useDeck } from "../deck/DeckContext";
import {
  BUILTIN_OPEN_FOLDER,
  bindingStepLabel,
  folderChildPageId,
  normalizeBinding,
} from "../lib/binding";
import type { Slot } from "../lib/types";

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
        registerPiFrameRef={registerPiFrameRef}
        onSettingsChange={saveNativeSettings}
        nativeSettingsVisible={piNativeSettingsVisible}
        settingsJson={piSettingsJson}
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
  registerPiFrameRef,
  onSettingsChange,
  nativeSettingsVisible,
  settingsJson,
}: {
  hidden: boolean;
  binding: ReturnType<typeof normalizeBinding> | undefined;
  registerPiFrameRef: (el: HTMLIFrameElement | null) => void;
  onSettingsChange: (json: string) => void;
  nativeSettingsVisible: boolean;
  settingsJson: string;
}) {
  const isCompanion = binding?.type === "companion";
  const [localJson, setLocalJson] = useState(settingsJson);

  useEffect(() => {
    setLocalJson(settingsJson);
  }, [settingsJson, binding?.actionId, binding?.actionUuid]);

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
      <iframe
        ref={registerPiFrameRef}
        id="pi-frame"
        title="Property Inspector"
        sandbox="allow-scripts allow-same-origin"
        className={nativeSettingsVisible ? "hidden" : undefined}
      />
      <div
        className={`native-settings${nativeSettingsVisible ? "" : " hidden"}`}
        id="native-settings"
      >
        <label>
          Settings JSON
          <textarea
            id="settings-json"
            rows={4}
            style={{ width: "100%" }}
            value={localJson}
            onChange={(e) => {
              setLocalJson(e.target.value);
              onSettingsChange(e.target.value);
            }}
          />
        </label>
      </div>
    </div>
  );
}
