export const toasts = $state([]);

let nextId = 0;

export function notify(kind, title, text = "") {
  const id = ++nextId;
  toasts.push({ id, kind, title, text });
  while (toasts.length > 3) toasts.shift();
  setTimeout(() => dismissToast(id), 3800);
}

export function dismissToast(id) {
  const idx = toasts.findIndex((t) => t.id === id);
  if (idx >= 0) toasts.splice(idx, 1);
}
