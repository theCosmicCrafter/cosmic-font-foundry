import { settings, inTauri } from "./settings.svelte.js";

// status: idle | checking | latest | available | downloading | installing | error
export const updater = $state({
  status: "idle",
  version: "",
  downloaded: 0,
  total: 0,
  error: "",
  portable: false,
  dismissed: false,
});

let pendingUpdate = null;

export async function checkForUpdates(manual = false) {
  if (!inTauri) return;
  const { invoke } = await import("@tauri-apps/api/core");
  const { check } = await import("@tauri-apps/plugin-updater");
  if (manual) updater.status = "checking";
  try {
    updater.portable = await invoke("is_portable");
    pendingUpdate = await check();
    if (pendingUpdate) {
      updater.version = pendingUpdate.version;
      updater.status = "available";
      updater.dismissed = false;
    } else if (manual) {
      updater.status = "latest";
    }
  } catch (e) {
    if (manual) {
      updater.error = String(e);
      updater.status = "error";
    } else {
      updater.status = "idle";
    }
  }
}

export async function installAppUpdate() {
  const { invoke } = await import("@tauri-apps/api/core");
  if (updater.portable) return invoke("open_web", { target: "release" });
  if (!pendingUpdate) return;
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await invoke("mark_show_on_open").catch(() => {});
  updater.status = "downloading";
  updater.downloaded = 0;
  updater.total = 0;
  try {
    await pendingUpdate.downloadAndInstall((e) => {
      if (e.event === "Started") updater.total = e.data.contentLength ?? 0;
      else if (e.event === "Progress") updater.downloaded += e.data.chunkLength;
      else if (e.event === "Finished") updater.status = "installing";
    });
    await relaunch();
  } catch (e) {
    updater.error = String(e);
    updater.status = "error";
  }
}

export function initUpdater() {
  if (inTauri && settings.checkUpdates) {
    setTimeout(() => checkForUpdates(false), 2500);
  }
}
