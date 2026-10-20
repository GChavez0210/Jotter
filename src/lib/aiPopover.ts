export function clampAiPopoverPosition(
  position: { left: number; top: number },
  menu: { width: number; height: number },
  viewport: { width: number; height: number }
): { left: number; top: number } {
  const margin = 12;
  return {
    left: Math.max(margin, Math.min(position.left, viewport.width - menu.width - margin)),
    top: Math.max(margin, Math.min(position.top, viewport.height - menu.height - margin)),
  };
}

/** Center the menu in the app window while keeping its controls within reach. */
export function aiPopoverPosition(
  menu: { width: number; height: number },
  viewport: { width: number; height: number }
): { left: number; top: number } {
  return clampAiPopoverPosition({
    left: (viewport.width - menu.width) / 2,
    top: (viewport.height - menu.height) / 2,
  }, menu, viewport);
}
