/**
 * Programmatic design token references matching `src/styles/design-tokens.css`.
 */
export const designTokens = {
  fonts: {
    ui: 'var(--font-ui)',
    display: 'var(--font-display)',
    mono: 'var(--font-mono)',
  },
  typeScale: {
    xs: '12px',
    sm: '14px',
    base: '16px',
    lg: '20px',
    xl: '28px',
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
    card: '8px',
  },
  layout: {
    controlMinHeight: 32,
    editorMaxWidth: 700,
    sidebarDefaultWidth: 260,
    sidebarMinWidth: 180,
    sidebarMaxWidth: 480,
  },
  motion: {
    fast: '140ms ease-out',
    panel: '180ms ease-out',
  },
} as const;
