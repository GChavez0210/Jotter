/** Unsaved Settings fields survive dialog changes only for this app session. */
export const settingsDrafts = $state({
  aiKey: '',
  style: null as string | null,
  caldavSource: '',
  caldavUrl: '',
  caldavUser: '',
  caldavPassword: '',
});
