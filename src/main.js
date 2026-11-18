import { mount } from "svelte";
import "@fontsource/archivo/latin-400.css";
import "@fontsource/archivo/latin-500.css";
import "@fontsource/archivo/latin-600.css";
import "@fontsource/archivo/latin-700.css";
import "@fontsource/jetbrains-mono/latin-400.css";
import "@fontsource/jetbrains-mono/latin-500.css";
import "./app.css";
import { initTooltips } from "./lib/tooltips.js";

document.addEventListener("contextmenu", (e) => {
  if (!e.target.closest("input, textarea")) e.preventDefault();
});
if (import.meta.env.PROD) {
  document.addEventListener("keydown", (e) => {
    const k = e.key.toLowerCase();
    if (k === "f5" || k === "f7" || (e.ctrlKey && ["r", "p", "g", "u", "j"].includes(k))) e.preventDefault();
  });
}

const mode = window.__COSMIC__?.mode;
if (mode === "menu") {
  import("./Menu.svelte").then(({ default: Menu }) => mount(Menu, { target: document.getElementById("app") }));
} else if (mode === "quick") {
  import("./QuickSwitcher.svelte").then(({ default: QuickSwitcher }) =>
    mount(QuickSwitcher, { target: document.getElementById("app") }),
  );
} else {
  initTooltips();
  import("./App.svelte").then(({ default: App }) => mount(App, { target: document.getElementById("app") }));
}
