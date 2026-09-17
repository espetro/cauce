export type ThemeChoice = "system" | "light" | "dark";

export const THEME_KEY = "oxe-theme";
export const THEMES: ThemeChoice[] = ["system", "light", "dark"];

/** Stored theme choice (default system). */
export function getTheme(): ThemeChoice {
  const v = localStorage.getItem(THEME_KEY);
  return v === "light" || v === "dark" ? v : "system";
}

export function setTheme(choice: ThemeChoice): void {
  localStorage.setItem(THEME_KEY, choice);
  applyTheme(choice);
}

/** Apply via daisyUI `data-theme`; system removes the attr so the
 * `color-scheme: light dark` media behavior takes over (prefersdark). */
export function applyTheme(choice: ThemeChoice): void {
  if (choice === "system") {
    document.documentElement.removeAttribute("data-theme");
  } else {
    document.documentElement.setAttribute("data-theme", choice);
  }
}

/** Apply the persisted choice once at app start. */
export function initTheme(): void {
  applyTheme(getTheme());
}
