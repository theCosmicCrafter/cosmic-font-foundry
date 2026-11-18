import { settings } from "./settings.svelte.js";
import en from "../i18n/en.js";
import es from "../i18n/es.js";

const DICTIONARIES = { en, es };

export const language = () => (settings.language === "es" ? "es" : "en");

export function t(key, vars) {
  let text = DICTIONARIES[language()][key] ?? en[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      text = text.replaceAll(`{${k}}`, v);
    }
  }
  return text;
}

export const locale = () => (language() === "es" ? "es-ES" : "en-US");

$effect.root(() => {
  $effect(() => {
    document.documentElement.lang = language();
  });
});
