import { appStorage } from '../appStorage';
import { defaultCaptureShortcut, defaultSummonShortcut } from '../platform';
import { m } from '../i18n.svelte';
import { normalizeTemplatesFolder } from '../templates';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import { rightPanelFitsWindow, sidebarFitsWindow, splitFitsWindow, titleBarIsCompact, widthForSidebar } from '../layout';
import { designTokens, isAccentPresetId, type AccentPresetId } from '../designTokens';
import type {
  CloseAction,
  CopyMode,
  EditingMode,
  FocusMode,
  LeftSidebarView,
  PaneId,
  RightPanelTab,
  SplitMode,
  ThemePreference,
} from '../types';

export const COPY_MODE_LABELS: Record<CopyMode, { label: string }> = {
  markdown_source: {
    get label() { return m.copy_mode_markdown(); },
  },
  plain: {
    get label() { return m.copy_mode_plain(); },
  },
  rich: {
    get label() { return m.copy_mode_rich(); },
  },
};

interface SavedPreFocusLayout {
  sidebarCollapsed: boolean;
  showInspector: boolean;
  showRightPanel: boolean;
}

type AccentMode = 'preset' | 'system' | 'custom';
export type WritingFont = 'serif' | 'sans' | 'mono';

export class SettingsStore {
  copyMode = $state<CopyMode>('markdown_source');
  themePref = $state<ThemePreference>('system');
  systemIsDark = $state(false);
  // Milestone 3: Editing Mode, Focus Mode & Focus Options
  editingMode = $state<EditingMode>('live');
  focusMode = $state<FocusMode>('off');
  typewriterScrolling = $state(false);
  dimInactiveParagraphs = $state(false);
  showCommandPalette = $state(false);
  showQuickSwitcher = $state(false);
  showTemplatePicker = $state(false);
  /** The folder a note made from the template picker goes into. */
  templatePickerFolder = $state('');
  topHoverReveal = $state(false);

  // Milestone 4: Left Sidebar Views & Right Backlinks Panel
  leftSidebarView = $state<LeftSidebarView>('files');
  selectedTag = $state<string | null>(null);
  showRightPanel = $state(true);
  rightPanelTab = $state<RightPanelTab>('note');

  /**
   * Split view. The mode and the note in the second pane last for the session, since open
   * notes do not survive a restart either; only the sash position is persisted.
   */
  splitMode = $state<SplitMode>('off');
  splitNotePath = $state<string | null>(null);
  splitRatio = $state(0.5);
  /** The pane that takes the next note the user opens, and whose editor commands act on. */
  activePane = $state<PaneId>('primary');

  // Milestone 5: Editor Font Size, Accent Color, Vault Hidden Files, Image Paste Format
  editorFontSize = $state<number>(16);
  accentColorMode = $state<AccentMode>('preset');
  accentPreset = $state<AccentPresetId>('ink');
  customAccentColor = $state<string>('#2f4fa8');
  showHiddenFiles = $state<boolean>(false);
  imageLinkFormat = $state<'wikilink' | 'markdown'>('wikilink');
  writingFont = $state<WritingFont>('serif');

  spellCheck = $state(true);
  showLineNumbers = $state(false);
  enabledSnippets = $state<string[]>([]);
  dailyFolder = $state('Daily');
  dailyTemplatePath = $state('Templates/Daily.md');
  templatesFolder = $state('Templates');
  quickCaptureEnabled = $state(false);
  quickCaptureShortcut = $state(defaultCaptureShortcut);
  /** The note quick capture appends to, relative to the vault. */
  quickCaptureNote = $state('Inbox.md');
  summonEnabled = $state(false);
  summonShortcut = $state(defaultSummonShortcut);
  /** Keep a tray icon while Jotter runs. */
  trayEnabled = $state(false);
  /** Notify when a task with a due date comes due. */
  taskReminders = $state(true);
  /** Keep a copy of a note's text before each save, so earlier versions can be restored. */
  versionHistory = $state(true);
  /** The note whose version history the timeline shows, or `null` while it is closed. */
  versionHistoryNote = $state<string | null>(null);
  /** Hiding to the tray needs the tray icon, so the two settings move together. */
  closeAction = $state<CloseAction>('quit');

  /** The Notes sidebar view lists folders, or tags with each note's links. */
  notesNavigation = $state<'folders' | 'tags'>('folders');
  /** Show note cards instead of an empty screen when no note is open. */
  showHomePage = $state(true);

  sidebarWidth = $state<number>(designTokens.layout.sidebarDefaultWidth);
  sidebarCollapsed = $state(false);
  /**
   * Current window width, read at startup so the first paint already knows whether the sidebar
   * fits. The resize handler keeps it current. Never persisted.
   */
  windowWidth = $state<number>(
    typeof window === 'undefined' ? designTokens.layout.windowMinWidth : window.innerWidth
  );
  showSettings = $state(false);
  /** Section the Settings dialog shows when it next opens, such as `ai`. */
  settingsSectionRequest = $state<string | null>(null);
  showInspector = $state(false);

  private preFocusLayout: SavedPreFocusLayout | null = null;

  effectiveDark = $derived(
    this.themePref === 'system' ? this.systemIsDark : this.themePref !== 'light'
  );

  /**
   * The window is too narrow for the sidebar and the text column together. Kept apart from
   * `sidebarCollapsed` so a narrow session never rewrites what the user chose.
   */
  sidebarAutoHidden = $derived(!sidebarFitsWindow(this.windowWidth, this.sidebarWidth));

  /** True while the sidebar belongs on screen, whoever hid it. */
  sidebarVisible = $derived(!this.sidebarCollapsed && !this.sidebarAutoHidden);

  /** The right panel yields its width before the sidebar and the second pane do. */
  rightPanelAutoHidden = $derived(
    !rightPanelFitsWindow(
      this.windowWidth,
      this.sidebarWidth,
      !this.sidebarCollapsed,
      this.splitMode !== 'off'
    )
  );

  /** True while the right panel belongs on screen, whoever hid it. */
  rightPanelVisible = $derived(this.showRightPanel && !this.rightPanelAutoHidden);

  /** The window is too narrow for two text columns. Widening it brings the split back. */
  splitAutoHidden = $derived(
    !splitFitsWindow(this.windowWidth, this.sidebarWidth, !this.sidebarCollapsed)
  );

  /** True while the second pane belongs on screen. Focus modes show one note only. */
  splitVisible = $derived(
    this.splitMode !== 'off' && this.focusMode === 'off' && !this.splitAutoHidden
  );

  effectivePane = $derived<PaneId>(
    this.splitVisible && this.splitMode === 'note' && this.activePane === 'secondary'
      ? 'secondary' : 'primary'
  );

  /** The title bar drops its text labels to keep the window controls reachable. */
  titleBarCompact = $derived(titleBarIsCompact(this.windowWidth));

  setCopyMode(next: CopyMode, onAfterChange?: () => void) {
    this.copyMode = next;
    try {
      appStorage.setItem('jotter.copyMode', next);
    } catch {
      // ignore storage errors
    }
    onAfterChange?.();
  }

  setEditingMode(next: EditingMode) {
    this.editingMode = next;
    try {
      appStorage.setItem('jotter.editingMode', next);
    } catch {
      // ignore
    }
  }

  toggleEditingMode() {
    this.setEditingMode(this.editingMode === 'live' ? 'source' : 'live');
  }

  setTypewriterScrolling(enabled: boolean) {
    this.typewriterScrolling = enabled;
    try {
      appStorage.setItem('jotter.typewriterScrolling', String(enabled));
    } catch {
      // ignore
    }
  }

  setDimInactiveParagraphs(enabled: boolean) {
    this.dimInactiveParagraphs = enabled;
    try {
      appStorage.setItem('jotter.dimInactiveParagraphs', String(enabled));
    } catch {
      // ignore
    }
  }

  setEditorFontSize(sizePx: number) {
    const allowed = [12, 14, 16, 20];
    const next = allowed.includes(sizePx) ? sizePx : 16;
    this.editorFontSize = next;
    document.documentElement.style.setProperty('--editor-font-size', `${next}px`);
    try {
      appStorage.setItem('jotter.editorFontSize', String(next));
    } catch {
      // ignore
    }
  }

  setAccentColor(mode: AccentMode, value?: string) {
    this.accentColorMode = mode;
    if (mode === 'preset' && value && isAccentPresetId(value)) {
      this.accentPreset = value;
    } else if (mode === 'custom' && value) {
      this.customAccentColor = value;
    }
    this.applyAccentToDocument();
    try {
      appStorage.setItem('jotter.accentMode', this.accentColorMode);
      appStorage.setItem('jotter.accentPreset', this.accentPreset);
      appStorage.setItem('jotter.customAccentColor', this.customAccentColor);
    } catch {
      // ignore
    }
  }

  applyAccentToDocument() {
    const root = document.documentElement;
    root.style.removeProperty('--accent');
    root.style.removeProperty('--on-accent');
    delete root.dataset.accent;
    if (this.accentColorMode === 'preset') {
      // Ink is the token default, so it needs no attribute.
      if (this.accentPreset !== 'ink') root.dataset.accent = this.accentPreset;
    } else if (this.accentColorMode === 'system') {
      root.style.setProperty('--accent', 'AccentColor');
      root.style.setProperty('--on-accent', 'AccentColorText');
    } else if (this.customAccentColor) {
      root.style.setProperty('--accent', this.customAccentColor);
      root.style.setProperty('--on-accent', '#ffffff');
    }
  }

  setShowHiddenFiles(show: boolean) {
    this.showHiddenFiles = show;
    try {
      appStorage.setItem('jotter.showHiddenFiles', String(show));
    } catch {
      // ignore
    }
  }

  setImageLinkFormat(fmt: 'wikilink' | 'markdown') {
    this.imageLinkFormat = fmt;
    try {
      appStorage.setItem('jotter.imageLinkFormat', fmt);
    } catch {
      // ignore
    }
  }

  /** Font for note text; titles stay in the display serif. Applied through `data-font` on the root. */
  setWritingFont(font: WritingFont) {
    this.writingFont = font;
    this.applyWritingFontToDocument();
    try {
      appStorage.setItem('jotter.writingFont', font);
    } catch {
      // ignore
    }
  }

  applyWritingFontToDocument() {
    const root = document.documentElement;
    if (this.writingFont === 'serif') delete root.dataset.font;
    else root.dataset.font = this.writingFont;
  }

  openSettings(section?: string) {
    this.settingsSectionRequest = section ?? null;
    this.showCommandPalette = false;
    this.showQuickSwitcher = false;
    this.showTemplatePicker = false;
    this.showSettings = true;
  }

  setNotesNavigation(next: 'folders' | 'tags') {
    this.notesNavigation = next;
    this.persist('jotter.notesNavigation', next);
  }

  setShowHomePage(enabled: boolean) {
    this.showHomePage = enabled;
    this.persist('jotter.showHomePage', String(enabled));
  }

  setSpellCheck(enabled: boolean) {
    this.spellCheck = enabled;
    this.persist('jotter.spellCheck', String(enabled));
  }

  setShowLineNumbers(enabled: boolean) {
    this.showLineNumbers = enabled;
    this.persist('jotter.showLineNumbers', String(enabled));
  }

  setSnippetEnabled(name: string, enabled: boolean) {
    const rest = this.enabledSnippets.filter((n) => n !== name);
    this.enabledSnippets = enabled ? [...rest, name] : rest;
    this.persist('jotter.enabledSnippets', JSON.stringify(this.enabledSnippets));
  }

  setDailyFolder(folder: string) {
    this.dailyFolder = folder.trim().replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
    this.persist('jotter.dailyFolder', this.dailyFolder);
  }

  setDailyTemplatePath(path: string) {
    this.dailyTemplatePath = path.trim().replace(/\\/g, '/').replace(/^\/+/, '');
    this.persist('jotter.dailyTemplatePath', this.dailyTemplatePath);
  }

  openTemplatePicker(folder = '') {
    this.templatePickerFolder = folder;
    this.showTemplatePicker = true;
  }

  setTemplatesFolder(folder: string) {
    this.templatesFolder = normalizeTemplatesFolder(folder);
    this.persist('jotter.templatesFolder', this.templatesFolder);
  }

  setSummon(enabled: boolean, shortcut = this.summonShortcut) {
    this.summonEnabled = enabled;
    this.summonShortcut = shortcut.trim() || defaultSummonShortcut;
    this.persist('jotter.summonEnabled', String(enabled));
    this.persist('jotter.summonShortcut', this.summonShortcut);
  }

  setQuickCapture(enabled: boolean, shortcut = this.quickCaptureShortcut) {
    this.quickCaptureEnabled = enabled;
    this.quickCaptureShortcut = shortcut.trim() || defaultCaptureShortcut;
    this.persist('jotter.quickCaptureEnabled', String(enabled));
    this.persist('jotter.quickCaptureShortcut', this.quickCaptureShortcut);
  }

  setQuickCaptureNote(path: string) {
    this.quickCaptureNote = path.trim().replace(/\\/g, '/').replace(/^\/+/, '') || 'Inbox.md';
    this.persist('jotter.quickCaptureNote', this.quickCaptureNote);
  }

  /** Loads and applies only the look of the app, for secondary windows that must not touch the window. */
  initAppearance() {
    const theme = appStorage.getItem('jotter.themePref');
    if (theme === 'system' || theme === 'light' || theme === 'dark' || theme === 'high-contrast') {
      this.themePref = theme;
    }
    const mode = appStorage.getItem('jotter.accentMode');
    if (mode === 'preset' || mode === 'system' || mode === 'custom') this.accentColorMode = mode;
    const preset = appStorage.getItem('jotter.accentPreset');
    if (isAccentPresetId(preset)) this.accentPreset = preset;
    const hex = appStorage.getItem('jotter.customAccentColor');
    if (hex && /^#[0-9a-fA-F]{6}$/.test(hex)) this.customAccentColor = hex;
    const font = appStorage.getItem('jotter.writingFont');
    if (font === 'serif' || font === 'sans' || font === 'mono') this.writingFont = font;
    this.applyThemeToDocument(this.themePref);
    this.applyAccentToDocument();
    this.applyWritingFontToDocument();
    const dark = window.matchMedia('(prefers-color-scheme: dark)');
    this.systemIsDark = dark.matches;
    if (!this.watchingSystemTheme) {
      this.watchingSystemTheme = true;
      dark.addEventListener('change', (e) => {
        this.systemIsDark = e.matches;
        this.applyThemeToDocument(this.themePref);
      });
    }
  }
  private watchingSystemTheme = false;

  setTaskReminders(enabled: boolean) {
    this.taskReminders = enabled;
    this.persist('jotter.taskReminders', String(enabled));
  }

  /** Turning the tray off sends the close button back to quitting. */
  setTrayEnabled(enabled: boolean) {
    this.trayEnabled = enabled;
    this.persist('jotter.trayEnabled', String(enabled));
    if (!enabled && this.closeAction === 'tray') this.setCloseAction('quit');
  }

  /** Hiding to the tray turns the tray icon on, so the window stays reachable. */
  setCloseAction(action: CloseAction) {
    this.closeAction = action;
    this.persist('jotter.closeAction', action);
    if (action === 'tray' && !this.trayEnabled) this.setTrayEnabled(true);
  }

  private persist(key: string, value: string) {
    try {
      appStorage.setItem(key, value);
    } catch {
      // ignore storage errors
    }
  }

  setFocusMode(next: FocusMode) {
    if (this.focusMode === 'off' && next !== 'off') {
      this.preFocusLayout = {
        sidebarCollapsed: this.sidebarCollapsed,
        showInspector: this.showInspector,
        showRightPanel: this.showRightPanel,
      };
      try {
        appStorage.setItem(
          'jotter.preFocusLayout',
          JSON.stringify(this.preFocusLayout)
        );
      } catch {
        // ignore
      }
    } else if (next === 'off' && this.preFocusLayout) {
      this.sidebarCollapsed = this.preFocusLayout.sidebarCollapsed;
      this.showInspector = this.preFocusLayout.showInspector;
      this.showRightPanel = this.preFocusLayout.showRightPanel ?? true;
      this.preFocusLayout = null;
      try {
        appStorage.setItem(
          'jotter.sidebarCollapsed',
          String(this.sidebarCollapsed)
        );
        appStorage.setItem(
          'jotter.showRightPanel',
          String(this.showRightPanel)
        );
        appStorage.removeItem('jotter.preFocusLayout');
      } catch {
        // ignore
      }
    }

    this.focusMode = next;
    this.topHoverReveal = false;
    this.showSettings = false;
    try {
      appStorage.setItem('jotter.focusMode', next);
    } catch {
      // ignore
    }

    void this.syncWindowFullscreen(next === 'zen');
  }

  toggleMinimalFocus() {
    this.setFocusMode(this.focusMode === 'minimal' ? 'off' : 'minimal');
  }

  toggleZenFocus() {
    this.setFocusMode(this.focusMode === 'zen' ? 'off' : 'zen');
  }

  exitFocusMode() {
    if (this.focusMode !== 'off') {
      this.setFocusMode('off');
    }
  }

  private async syncWindowFullscreen(fullscreen: boolean) {
    try {
      const win = getCurrentWindow();
      await win.setFullscreen(fullscreen);
    } catch {
      // ignore when running in browser preview outside Tauri
    }
  }

  setThemePreference(next: ThemePreference) {
    this.themePref = next;
    try {
      appStorage.setItem('jotter.themePref', next);
    } catch {
      // ignore
    }
    this.applyThemeToDocument(next);
  }

  setSidebarWidth(width: number) {
    this.sidebarWidth = width;
    try {
      appStorage.setItem('jotter.sidebarWidth', String(width));
    } catch {
      // ignore
    }
  }

  /**
   * Shows or hides the sidebar, acting on what the user sees. Asking for it back in a window
   * that cannot fit it widens the window instead of refusing.
   */
  toggleSidebar() {
    const show = !this.sidebarVisible;
    this.setSidebarCollapsed(!show);
    if (show && this.sidebarAutoHidden) void this.widenWindowForSidebar();
  }

  private setSidebarCollapsed(collapsed: boolean) {
    this.sidebarCollapsed = collapsed;
    this.persist('jotter.sidebarCollapsed', String(collapsed));
  }

  /** Grows the window to the width the sidebar and the text column need, and no further. */
  private async widenWindowForSidebar() {
    await this.widenWindowTo(widthForSidebar(this.sidebarWidth));
  }

  private async widenWindowTo(target: number) {
    if (this.windowWidth >= target) return;
    try {
      const win = getCurrentWindow();
      await win.setSize(new LogicalSize(target, window.innerHeight));
    } catch {
      // ignore when running in browser preview outside Tauri
    }
  }

  /** Tracks the window width so the sidebar and the title bar can answer for themselves. */
  setWindowWidth(width: number) {
    this.windowWidth = width;
  }

  setLeftSidebarView(view: LeftSidebarView) {
    if (this.focusMode !== 'off') {
      this.exitFocusMode();
    }
    this.leftSidebarView = view;
    // Picking a view asks for the sidebar, so a narrow window widens to show it.
    if (!this.sidebarVisible) this.toggleSidebar();
    try {
      appStorage.setItem('jotter.leftSidebarView', view);
    } catch {
      // ignore
    }
  }

  /** Opens the right panel on a tab, widening a narrow window the way the sidebar does. */
  showRightPanelTab(tab: RightPanelTab) {
    this.rightPanelTab = tab;
    this.persist('jotter.rightPanelTab', tab);
    if (this.focusMode !== 'off') this.exitFocusMode();
    if (this.rightPanelVisible) return;
    this.showRightPanel = true;
    this.persist('jotter.showRightPanel', 'true');
    if (this.rightPanelAutoHidden) void this.widenWindowForRightPanel();
  }

  /** Like the sidebar, asking for the right panel in a narrow window widens the window. */
  toggleRightPanel() {
    const show = !this.rightPanelVisible;
    this.showRightPanel = show;
    this.persist('jotter.showRightPanel', String(show));
    if (show && this.rightPanelAutoHidden) void this.widenWindowForRightPanel();
  }

  /**
   * Opens the second pane on a note, or closes the split when that note is already there.
   * A window too narrow for two text columns widens, as asking for the sidebar does.
   */
  openSplitNote(relPath: string | null) {
    if (this.splitMode === 'note' && relPath && this.splitNotePath === relPath) {
      this.closeSplit();
      return;
    }
    this.splitMode = 'note';
    this.splitNotePath = relPath;
    this.activePane = 'secondary';
    if (this.focusMode !== 'off') this.exitFocusMode();
    if (this.splitAutoHidden) void this.widenWindowForSplit();
  }

  /** Shows the open note's rendered preview beside it. The preview takes no focus. */
  openSplitPreview() {
    this.splitMode = 'preview';
    this.splitNotePath = null;
    this.activePane = 'primary';
    if (this.focusMode !== 'off') this.exitFocusMode();
    if (this.splitAutoHidden) void this.widenWindowForSplit();
  }

  closeSplit() {
    this.splitMode = 'off';
    this.splitNotePath = null;
    this.activePane = 'primary';
  }

  setActivePane(pane: PaneId) {
    if (this.splitMode === 'off') {
      this.activePane = 'primary';
      return;
    }
    this.activePane = pane;
  }

  setVersionHistory(enabled: boolean) {
    this.versionHistory = enabled;
    this.persist('jotter.versionHistory', String(enabled));
  }

  /** Opens the timeline for a note. The dialog loads that note's versions when it appears. */
  openVersionHistory(relPath: string | null) {
    this.versionHistoryNote = relPath;
  }

  setSplitRatio(ratio: number) {
    if (!Number.isFinite(ratio)) return;
    this.splitRatio = Math.max(0.2, Math.min(0.8, ratio));
    this.persist('jotter.splitRatio', this.splitRatio.toFixed(3));
  }

  private async widenWindowForSplit() {
    const { editorMinWidth } = designTokens.layout;
    const target = (!this.sidebarCollapsed ? Math.round(this.sidebarWidth) : 0) + editorMinWidth * 2;
    await this.widenWindowTo(target);
  }

  private async widenWindowForRightPanel() {
    const { rightPanelWidth, editorMinWidth } = designTokens.layout;
    const target =
      (!this.sidebarCollapsed ? Math.round(this.sidebarWidth) : 0) + rightPanelWidth + editorMinWidth;
    await this.widenWindowTo(target);
  }

  applyThemeToDocument(pref: ThemePreference) {
    const root = document.documentElement;
    if (pref === 'system') {
      root.removeAttribute('data-theme');
    } else {
      root.setAttribute('data-theme', pref);
    }
  }

  initFromStorageAndSystem(): () => void {
    try {
      const savedMode = appStorage.getItem('jotter.copyMode') as CopyMode | null;
      if (
        savedMode === 'markdown_source' ||
        savedMode === 'plain' ||
        savedMode === 'rich'
      ) {
        this.copyMode = savedMode;
      }
      const savedTheme = appStorage.getItem(
        'jotter.themePref'
      ) as ThemePreference | null;
      if (
        savedTheme === 'system' ||
        savedTheme === 'light' ||
        savedTheme === 'dark' ||
        savedTheme === 'high-contrast'
      ) {
        this.themePref = savedTheme;
      }
      const savedWidth = Number(appStorage.getItem('jotter.sidebarWidth'));
      if (savedWidth >= 180 && savedWidth <= 480) {
        this.sidebarWidth = savedWidth;
      }
      const savedCollapsed = appStorage.getItem('jotter.sidebarCollapsed');
      if (savedCollapsed === 'true' || savedCollapsed === 'false') {
        this.sidebarCollapsed = savedCollapsed === 'true';
      }
      const savedLeftView = appStorage.getItem(
        'jotter.leftSidebarView'
      ) as LeftSidebarView | null;
      if (
        savedLeftView === 'files' ||
        savedLeftView === 'search' ||
        savedLeftView === 'tags'
      ) {
        this.leftSidebarView = savedLeftView;
      }
      // Calendar and Recently deleted moved from the sidebar to the right panel.
      const savedRightTab = appStorage.getItem('jotter.rightPanelTab');
      const legacyTab = savedLeftView as string | null;
      if (savedRightTab === 'note' || savedRightTab === 'calendar' || savedRightTab === 'trash') {
        this.rightPanelTab = savedRightTab;
      } else if (legacyTab === 'calendar' || legacyTab === 'trash') {
        this.rightPanelTab = legacyTab;
      }
      const savedRatio = Number(appStorage.getItem('jotter.splitRatio'));
      if (savedRatio >= 0.2 && savedRatio <= 0.8) {
        this.splitRatio = savedRatio;
      }
      const savedRightPanel = appStorage.getItem('jotter.showRightPanel');
      if (savedRightPanel === 'true' || savedRightPanel === 'false') {
        this.showRightPanel = savedRightPanel === 'true';
      }
      const savedEditingMode = appStorage.getItem(
        'jotter.editingMode'
      ) as EditingMode | null;
      if (savedEditingMode === 'live' || savedEditingMode === 'source') {
        this.editingMode = savedEditingMode;
      }
      const savedTypewriter = appStorage.getItem('jotter.typewriterScrolling');
      if (savedTypewriter === 'true' || savedTypewriter === 'false') {
        this.typewriterScrolling = savedTypewriter === 'true';
      }
      const savedDim = appStorage.getItem('jotter.dimInactiveParagraphs');
      if (savedDim === 'true' || savedDim === 'false') {
        this.dimInactiveParagraphs = savedDim === 'true';
      }
      const savedFontSize = Number(appStorage.getItem('jotter.editorFontSize'));
      if ([12, 14, 16, 20].includes(savedFontSize)) {
        this.editorFontSize = savedFontSize;
      }
      document.documentElement.style.setProperty(
        '--editor-font-size',
        `${this.editorFontSize}px`
      );
      const savedAccentMode = appStorage.getItem('jotter.accentMode');
      if (savedAccentMode === 'preset' || savedAccentMode === 'system' || savedAccentMode === 'custom') {
        this.accentColorMode = savedAccentMode;
      } else if (appStorage.getItem('jotter.accentColorMode') === 'custom') {
        // Keep a custom accent chosen before presets existed. The old 'system' default becomes Ink.
        this.accentColorMode = 'custom';
      }
      const savedPreset = appStorage.getItem('jotter.accentPreset');
      if (isAccentPresetId(savedPreset)) {
        this.accentPreset = savedPreset;
      }
      const savedAccentHex = appStorage.getItem('jotter.customAccentColor');
      if (savedAccentHex && /^#[0-9a-fA-F]{6}$/.test(savedAccentHex)) {
        this.customAccentColor = savedAccentHex;
      }
      this.applyAccentToDocument();
      const savedHidden = appStorage.getItem('jotter.showHiddenFiles');
      if (savedHidden === 'true' || savedHidden === 'false') {
        this.showHiddenFiles = savedHidden === 'true';
      }
      const savedImgFormat = appStorage.getItem('jotter.imageLinkFormat');
      if (savedImgFormat === 'wikilink' || savedImgFormat === 'markdown') {
        this.imageLinkFormat = savedImgFormat;
      }
      const savedFont = appStorage.getItem('jotter.writingFont');
      if (savedFont === 'serif' || savedFont === 'sans' || savedFont === 'mono') {
        this.writingFont = savedFont;
      }
      this.applyWritingFontToDocument();
      const savedNav = appStorage.getItem('jotter.notesNavigation');
      if (savedNav === 'folders' || savedNav === 'tags') this.notesNavigation = savedNav;
      const savedHome = appStorage.getItem('jotter.showHomePage');
      if (savedHome === 'true' || savedHome === 'false') this.showHomePage = savedHome === 'true';
      this.showLineNumbers = appStorage.getItem('jotter.showLineNumbers') === 'true';
      const savedSpell = appStorage.getItem('jotter.spellCheck');
      if (savedSpell === 'true' || savedSpell === 'false') {
        this.spellCheck = savedSpell === 'true';
      }
      try {
        const savedSnippets = JSON.parse(appStorage.getItem('jotter.enabledSnippets') ?? '[]');
        if (Array.isArray(savedSnippets)) {
          this.enabledSnippets = savedSnippets.filter((n) => typeof n === 'string');
        }
      } catch {
        this.enabledSnippets = [];
      }
      const savedDailyFolder = appStorage.getItem('jotter.dailyFolder');
      if (savedDailyFolder !== null) this.dailyFolder = savedDailyFolder;
      const savedDailyTemplate = appStorage.getItem('jotter.dailyTemplatePath');
      if (savedDailyTemplate !== null) this.dailyTemplatePath = savedDailyTemplate;
      const savedTemplatesFolder = appStorage.getItem('jotter.templatesFolder');
      if (savedTemplatesFolder !== null) this.templatesFolder = savedTemplatesFolder;
      this.taskReminders = appStorage.getItem('jotter.taskReminders') !== 'false';
      this.versionHistory = appStorage.getItem('jotter.versionHistory') !== 'false';
      this.quickCaptureEnabled = appStorage.getItem('jotter.quickCaptureEnabled') === 'true';
      const savedCaptureShortcut = appStorage.getItem('jotter.quickCaptureShortcut');
      if (savedCaptureShortcut) this.quickCaptureShortcut = savedCaptureShortcut;
      const savedCaptureNote = appStorage.getItem('jotter.quickCaptureNote');
      if (savedCaptureNote) this.quickCaptureNote = savedCaptureNote;
      this.summonEnabled = appStorage.getItem('jotter.summonEnabled') === 'true';
      const savedShortcut = appStorage.getItem('jotter.summonShortcut');
      if (savedShortcut) this.summonShortcut = savedShortcut;
      // The summon hotkey used to force tray residency and hide-on-close. Keep that behavior
      // for anyone who had it on before the two settings became independent.
      const savedTray = appStorage.getItem('jotter.trayEnabled');
      const savedCloseAction = appStorage.getItem('jotter.closeAction');
      if (savedTray === null && savedCloseAction === null) {
        this.trayEnabled = this.summonEnabled;
        this.closeAction = this.summonEnabled ? 'tray' : 'quit';
      } else {
        this.trayEnabled = savedTray === 'true';
        this.closeAction = savedCloseAction === 'tray' && this.trayEnabled ? 'tray' : 'quit';
      }
      // Record defaults too, so enabling summon later never looks like a legacy install.
      this.persist('jotter.trayEnabled', String(this.trayEnabled));
      this.persist('jotter.closeAction', this.closeAction);
      const savedPreFocus = appStorage.getItem('jotter.preFocusLayout');
      if (savedPreFocus) {
        try {
          this.preFocusLayout = JSON.parse(savedPreFocus) as SavedPreFocusLayout;
        } catch {
          this.preFocusLayout = null;
        }
      }
      const savedFocusMode = appStorage.getItem(
        'jotter.focusMode'
      ) as FocusMode | null;
      if (
        savedFocusMode === 'off' ||
        savedFocusMode === 'minimal' ||
        savedFocusMode === 'zen'
      ) {
        this.focusMode = savedFocusMode;
        if (savedFocusMode === 'zen') {
          void this.syncWindowFullscreen(true);
        }
      }
    } catch {
      // ignore
    }

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    this.systemIsDark = mediaQuery.matches;
    this.applyThemeToDocument(this.themePref);

    const handleMediaChange = (e: MediaQueryListEvent) => {
      this.systemIsDark = e.matches;
      this.applyThemeToDocument(this.themePref);
    };
    mediaQuery.addEventListener('change', handleMediaChange);

    const handleResize = () => this.setWindowWidth(window.innerWidth);
    handleResize();
    window.addEventListener('resize', handleResize);

    return () => {
      mediaQuery.removeEventListener('change', handleMediaChange);
      window.removeEventListener('resize', handleResize);
    };
  }
}

export const settingsStore = new SettingsStore();
