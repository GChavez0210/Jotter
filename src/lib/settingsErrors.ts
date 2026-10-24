import { m } from './i18n.svelte';

/** Native errors carry stable codes. OS diagnostic text is not UI copy. */
export function settingsErrorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'code' in error) {
    const { code } = error;
    if (code === 'too_large') return m.settings_file_too_large();
    if (code === 'read') return m.settings_file_read_failed();
    if (code === 'write') return m.settings_file_write_failed();
    if (code === 'invalid_path') return m.settings_file_path_failed();
  }
  return m.settings_storage_failed();
}
