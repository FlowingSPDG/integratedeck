import { getCurrentWindow } from "@tauri-apps/api/window";

function isTextInput(target: EventTarget | null | undefined): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
    return true;
  }
  if (target instanceof HTMLSelectElement) return true;
  return target.isContentEditable;
}

/** Suppress macOS WebView beep on keydown outside text fields. */
function installBeepWorkaround(): void {
  window.addEventListener("keydown", (event) => {
    if (event.altKey && event.key === "F4") return;
    const target = event.composedPath()[0];
    if (!isTextInput(target) && !event.defaultPrevented) {
      event.preventDefault();
    }
  });
}

/** Block context menu outside text fields and slot buttons (native app feel). */
function installContextMenuBlock(): void {
  window.addEventListener("contextmenu", (event) => {
    const target = event.composedPath()[0];
    if (isTextInput(target)) return;
    if (target instanceof HTMLElement && target.closest(".slot")) return;
    event.preventDefault();
  });
}

async function applyTheme(theme: "light" | "dark" | null | undefined): Promise<void> {
  document.documentElement.dataset.theme = theme === "light" ? "light" : "dark";
}

async function installThemeSync(): Promise<void> {
  const window = getCurrentWindow();
  await applyTheme(await window.theme());
  await window.onThemeChanged(({ payload }) => {
    void applyTheme(payload);
  });
}

export function installNativeFeel(): void {
  installBeepWorkaround();
  installContextMenuBlock();
  void installThemeSync();
}
