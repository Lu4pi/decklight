// Kurze Meldungen unten rechts.

export interface ToastMsg {
  id: number;
  text: string;
  kind: "info" | "error";
}

let next = 1;
export const toasts = $state<{ list: ToastMsg[] }>({ list: [] });

export function toast(text: string, kind: "info" | "error" = "info") {
  const id = next++;
  toasts.list = [...toasts.list, { id, text, kind }];
  setTimeout(() => (toasts.list = toasts.list.filter((t) => t.id !== id)), kind === "error" ? 6000 : 2500);
}
