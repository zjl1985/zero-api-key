export type Theme = "default" | "rose-pine";

const THEME_KEY = "zero-api-key:theme";

export function loadTheme(): Theme {
  return localStorage.getItem(THEME_KEY) === "rose-pine" ? "rose-pine" : "default";
}

export function saveTheme(theme: Theme): void {
  localStorage.setItem(THEME_KEY, theme);
}

export function applyTheme(theme: Theme): void {
  if (theme === "default") {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = theme;
  }
}
