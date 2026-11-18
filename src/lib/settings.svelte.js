import { LazyStore } from "@tauri-apps/plugin-store";
import { version as pkgVersion } from "../../package.json";

const boot = window.__COSMIC__ ?? { settings: {}, version: pkgVersion };

export const inTauri = "__TAURI_INTERNALS__" in window;
export const version = boot.version;

export const DEFAULT_SHORTCUTS = {
  search: "Ctrl+F",
  all: "Ctrl+A",
  toggle: "Space",
  quickSwitcher: "Ctrl+Shift+F",
};

const DEFAULTS = {
  theme: null,
  language: boot.systemLanguage ?? "en",
  languageChosen: false,
  installFor: "user", // "user" | "all"
  onOpen: "installed", // "installed" | "favorites"
  onClose: "tray", // "tray" | "exit"
  systemUnlocked: false,
  showLocked: false,
  checkUpdates: true,
  sampleText: "",
  shortcuts: { ...DEFAULT_SHORTCUTS },
  quickSwitcherShortcut: "Ctrl+Shift+F",
  view: "list", // "list" | "grid"
  size: 30,
  syncFolder: "",
  lastSyncAt: 0,
  showAgentTab: false,
};

export const settings = $state({ ...DEFAULTS, ...(boot.settings ?? {}) });
settings.shortcuts = { ...DEFAULT_SHORTCUTS, ...(settings.shortcuts || {}) };

export function setLanguage(lang) {
  settings.language = lang;
  settings.languageChosen = true;
}

const prefersDark = matchMedia("(prefers-color-scheme: dark)");
export const currentTheme = () => settings.theme ?? (prefersDark.matches ? "dark" : "light");

const store = inTauri ? new LazyStore("settings.json", { autoSave: 150 }) : null;

$effect.root(() => {
  let first = true;
  $effect(() => {
    const theme = currentTheme();
    const html = document.documentElement;
    if (first || html.dataset.theme === theme) {
      html.dataset.theme = theme;
      first = false;
      return;
    }
    html.classList.add("theme-switching");
    html.dataset.theme = theme;
    requestAnimationFrame(() => requestAnimationFrame(() => html.classList.remove("theme-switching")));
  });

  for (const key of Object.keys(DEFAULTS)) {
    $effect(() => {
      const value = $state.snapshot(settings[key]);
      store?.set(key, value);
    });
  }
});

export function setTheme(theme) {
  settings.theme = theme;
  if (!inTauri) return;
  import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
    getCurrentWindow().setBackgroundColor(theme === "dark" ? [0x09, 0x08, 0x0c, 255] : [0xfd, 0xf9, 0xf4, 255]),
  );
}
