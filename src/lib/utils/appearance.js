// Theme and light/dark mode switching for Skeleton v5.
//
// Both attributes live on <html>: the theme variables must be visible to the html
// element itself (Skeleton paints the page background there), and the Tailwind `dark`
// variant is defined in app.css against [data-mode=dark] on the root element.

// Skeleton renamed two of the presets between v2 and v5; stored preferences from older
// Hub versions still use the old names.
const RENAMED_THEMES = { skeleton: 'legacy', 'gold-nouveau': 'nouveau' };

export const DEFAULT_THEME = 'legacy';
export const DEFAULT_MODE = 'dark';

export const normalizeThemeName = (name) => RENAMED_THEMES[name] ?? name ?? DEFAULT_THEME;

export const applyTheme = (name) => {
    document.documentElement.dataset.theme = normalizeThemeName(name);
};

export const getAppliedTheme = () => document.documentElement.dataset.theme ?? DEFAULT_THEME;

export const applyMode = (mode) => {
    document.documentElement.dataset.mode = mode === 'light' ? 'light' : 'dark';
};
