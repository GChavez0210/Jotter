# Jotter design

Jotter uses a warm paper look of its own that renders the same on Windows, macOS and Linux. The clickable mockup is the visual reference: [Jotter Paper Mockup](https://claude.ai/artifact/YAzqtj8CG1or9MM4bk5xrH) (private; ask the owner for access).

## Principles

1. The note is the interface. Hairlines split regions; only popovers and dialogs float.
2. One accent, used for the active item, links, selection, focus and the primary button.
3. Typography carries the hierarchy. UI text is 13 px, note text 17 px, note titles 32 px.
4. Tools appear when needed. Formatting shows on selection; rare actions live in the command palette.
5. Two columns: sidebar and editor. There is no separate note-list column.

## Tokens

All colors, type, spacing, radii and motion live in [`src/styles/design-tokens.css`](../src/styles/design-tokens.css), mirrored in [`src/lib/designTokens.ts`](../src/lib/designTokens.ts). Components use tokens only, never literal colors or pixel font sizes.

| Group | Tokens |
| --- | --- |
| Surfaces | `--surface-app` (sidebar), `--surface-editor`, `--surface-raised` (popovers), `--surface-field`, `--surface-hover`, `--surface-active`, `--surface-subtle`, `--surface-code`, `--surface-mica` |
| Text | `--text-primary`, `--text-secondary`, `--text-muted`, `--text-danger` |
| Lines | `--line-subtle`, `--line-control`, `--line-strong` |
| Accent | `--accent`, `--on-accent`, `--accent-soft`, `--selection`, `--mark` |
| Type | `--font-ui` (Instrument Sans), `--font-display` (Literata), `--font-body` (writing font), `--font-mono` (JetBrains Mono); `--fs-11` to `--fs-32` |

The scale is 11, 12, 13, 15, 17, 24 and 32 px. Use 11 px only for meta text such as counts and timestamps.

Themes switch with `data-theme` on the root element (`light`, `dark`, or none for the system setting). Accent presets switch with `data-accent`: `ink` (default), `moss`, `plum`, `teal`, `rust`, `graphite`, `amber`, `rose`. Each preset has a light and a dark value.

Fonts ship with the app through `@fontsource-variable` packages imported in [`src/main.ts`](../src/main.ts). All three use the SIL Open Font License.

## Sample vault

[`sample-vault/`](sample-vault/) holds the notes from the mockup. Open it in Jotter for screenshots, so before-and-after comparisons never show a real vault.
