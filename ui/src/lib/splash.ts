// Startbildschirm aus index.html: Status anzeigen und ausblenden, sobald der
// Kern Zustand liefert. Kurz stehen lassen, damit die Animation nicht flackert;
// nach spätestens MAX_MS weiter, die App zeigt dann selbst „getrennt“.

const MIN_MS = 2400;
const MAX_MS = 9000;

const el = () => document.getElementById("splash");

export function splashStatus(text: string) {
  const s = document.getElementById("splash-status");
  if (s && s.textContent !== text) s.textContent = text;
}

let gone = false;

/** Startbildschirm ausblenden. `instant` für Ansichten ohne Startbildschirm (Monitor, Overlay). */
export function hideSplash(instant = false) {
  if (gone) return;
  const s = el();
  if (!s) return;
  if (instant) {
    gone = true;
    s.remove();
    return;
  }
  const wait = Math.max(0, MIN_MS - performance.now());
  setTimeout(() => {
    if (gone) return;
    gone = true;
    s.classList.add("out");
    setTimeout(() => s.remove(), 600);
  }, wait);
}

/** Sicherheitsnetz: nie länger als MAX_MS auf den Kern warten. */
export function splashTimeout() {
  setTimeout(() => hideSplash(), MAX_MS);
}
