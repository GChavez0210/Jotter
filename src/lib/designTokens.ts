/**
 * Programmatic design token references matching `src/styles/design-tokens.css`.
 */
export const designTokens = {
  fonts: {
    ui: 'var(--font-ui)',
    display: 'var(--font-display)',
    body: 'var(--font-body)',
    mono: 'var(--font-mono)',
  },
  typeScale: {
    meta: '11px',
    small: '12px',
    ui: '13px',
    reading: '15px',
    body: '17px',
    heading: '24px',
    title: '32px',
  },
  spacing: {
    s1: '4px',
    s2: '8px',
    s3: '12px',
    s4: '16px',
    s5: '20px',
    s6: '24px',
    s8: '32px',
  },
  radii: {
    control: '4px',
    card: '6px',
    dialog: '10px',
  },
  layout: {
    controlMinHeight: 28,
    rowHeight: 28,
    barHeight: 40,
    editorMaxWidth: 680,
    sidebarDefaultWidth: 248,
    sidebarMinWidth: 180,
    sidebarMaxWidth: 480,
  },
  motion: {
    fast: '120ms ease-out',
    panel: '180ms ease-out',
  },
} as const;

/** Accent presets. `light` and `dark` mirror the `[data-accent]` blocks in design-tokens.css. */
export const ACCENT_PRESETS = [
  { id: 'ink', label: 'Ink', light: '#2f4fa8', dark: '#93a9ee' },
  { id: 'moss', label: 'Moss', light: '#3d6b35', dark: '#9cc48f' },
  { id: 'plum', label: 'Plum', light: '#7b3f78', dark: '#d6a3d3' },
  { id: 'teal', label: 'Teal', light: '#1d6b6c', dark: '#7ccbc9' },
  { id: 'rust', label: 'Rust', light: '#a3472a', dark: '#e59a7c' },
  { id: 'graphite', label: 'Graphite', light: '#46443f', dark: '#cfcac0' },
  { id: 'amber', label: 'Amber', light: '#8a5d08', dark: '#e3b65a' },
  { id: 'rose', label: 'Rose', light: '#a6375a', dark: '#ee9db5' },
] as const;

export type AccentPresetId = (typeof ACCENT_PRESETS)[number]['id'];

export function isAccentPresetId(value: string | null): value is AccentPresetId {
  return ACCENT_PRESETS.some((preset) => preset.id === value);
}
