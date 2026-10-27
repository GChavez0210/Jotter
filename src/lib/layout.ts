import { designTokens } from './designTokens';

/**
 * Window width that fits the sidebar beside a readable text column. The settings dialog and the
 * window resize handler both measure against this, so one rule decides when the sidebar fits.
 */
export function widthForSidebar(sidebarWidth: number): number {
  return Math.round(sidebarWidth) + designTokens.layout.editorMinWidth;
}

/** True while the window has room for the sidebar and the text column side by side. */
export function sidebarFitsWindow(windowWidth: number, sidebarWidth: number): boolean {
  return windowWidth >= widthForSidebar(sidebarWidth);
}

/**
 * True while the window has room for the right panel beside the text column, counting the
 * sidebar whenever the user wants it open, including while it is automatically hidden.
 * The right panel yields first, so a narrowing window keeps the file tree longer. A split
 * asks for a second text column, so the panel yields that much sooner.
 */
export function rightPanelFitsWindow(
  windowWidth: number,
  sidebarWidth: number,
  sidebarRequested: boolean,
  splitRequested = false
): boolean {
  const { rightPanelWidth } = designTokens.layout;
  const taken = sidebarRequested ? Math.round(sidebarWidth) : 0;
  return windowWidth >= taken + rightPanelWidth + editorArea(splitRequested);
}

/**
 * True while the window has room for two text columns beside the sidebar. The right panel
 * has already yielded by this point, so it counts for nothing here.
 */
export function splitFitsWindow(
  windowWidth: number,
  sidebarWidth: number,
  sidebarRequested: boolean
): boolean {
  const taken = sidebarRequested ? Math.round(sidebarWidth) : 0;
  return windowWidth >= taken + editorArea(true);
}

/** Width the editor area needs: one readable text column, or two while a split is open. */
function editorArea(split: boolean): number {
  return designTokens.layout.editorMinWidth * (split ? 2 : 1);
}

/**
 * Share of the editor area the left pane may take, so neither pane falls below a readable
 * text column. A row with room for exactly two columns pins the sash to the middle.
 */
export function clampSplitRatio(ratio: number, rowWidth: number): number {
  const smallest = designTokens.layout.editorMinWidth / Math.max(rowWidth, 1);
  if (smallest >= 0.5) return 0.5;
  return Math.max(smallest, Math.min(1 - smallest, ratio));
}

/**
 * Widest the sidebar may be dragged in this window. Without the ceiling, widening the sidebar
 * past what the window holds would hide the sidebar under the pointer that is dragging it.
 */
export function maxSidebarWidth(windowWidth: number): number {
  const { sidebarMinWidth, sidebarMaxWidth, editorMinWidth } = designTokens.layout;
  return Math.max(sidebarMinWidth, Math.min(sidebarMaxWidth, windowWidth - editorMinWidth));
}

/** True while the window is too narrow for the title bar to carry its text labels. */
export function titleBarIsCompact(windowWidth: number): boolean {
  return windowWidth < designTokens.layout.titleBarCompactWidth;
}
