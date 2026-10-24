/** Wait for notes and settings before closing or reloading the WebView. */
export async function saveBeforeLeaving(
  saveNotes: () => Promise<boolean>,
  saveSettings: () => Promise<void>,
): Promise<boolean> {
  if (!(await saveNotes())) return false;
  await saveSettings();
  return true;
}
