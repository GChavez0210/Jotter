import { defaultSummonShortcut } from '../platform';
import { m } from '../i18n.svelte';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { designTokens, isAccentPresetId, type AccentPresetId } from '../designTokens';
import type {
  CloseAction,
  CopyMode,
  EditingMode,
  FocusMode,
  LeftSidebarView,
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

class SettingsStore {
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
  topHoverReveal = $state(false);

  // Milestone 4: Left Sidebar Views & Right Backlinks Panel
  leftSidebarView = $state<LeftSidebarView>('files');
  selectedTag = $state<string | null>(null);
  showRightPanel = $state(true);

  // Milestone 5: Editor Font Size, Accent Color, Vault Hidden Files, Image Paste Format
  editorFontSize = $state<number>(16);
  accentColorMode = $state<AccentMode>('preset');
  accentPreset = $state<AccentPresetId>('ink');
  customAccentColor = $state<string>('#2f4fa8');
  showHiddenFiles = $state<boolean>(false);
  imageLinkFormat = $state<'wikilink' | 'markdown'>('wikilink');
  writingFont = $state<WritingFont>('serif');

  // Roadmap phase 1
  spellCheck = $state(true);
  showLineNumbers = $state(false);
  enabledSnippets = $state<string[]>([]);
  dailyFolder = $state('Daily');
  dailyTemplatePath = $state('Templates/Daily.md');
  summonEnabled = $state(false);
  summonShortcut = $state(defaultSummonShortcut);
  /** Keep a tray icon while Jotter runs. */
  trayEnabled = $state(false);
  /** Hiding to the tray needs the tray icon, so the two settings move together. */
  closeAction = $state<CloseAction>('quit');

  // Roadmap phase 2
  /** The Notes sidebar view lists folders, or tags with each note's links. */
  notesNavigation = $state<'folders' | 'tags'>('folders');
  /** Show note cards instead of an empty screen when no note is open. */
  showHomePage = $state(true);

  sidebarWidth = $state<number>(designTokens.layout.sidebarDefaultWidth);
  sidebarCollapsed = $state(false);
  showSettings = $state(false);
  /** Section the Settings dialog shows when it next opens, such as `ai`. */
  settingsSectionRequest = $state<string | null>(null);
  showInspector = $state(false);

  private preFocusLayout: SavedPreFocusLayout | null = null;

  effectiveDark = $derived(
    this.themePref === 'system' ? this.systemIsDark : this.themePref !== 'light'
  );

  setCopyMode(next: CopyMode, onAfterChange?: () => void) {
    this.copyMode = next;
    try {
      localStorage.setItem('jotter.copyMode', next);
    } catch {
      // ignore storage errors
    }
    onAfterChange?.();
  }

  setEditingMode(next: EditingMode) {
    this.editingMode = next;
    try {
      localStorage.setItem('jotter.editingMode', next);
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
      localStorage.setItem('jotter.typewriterScrolling', String(enabled));
    } catch {
      // ignore
    }
  }

  setDimInactiveParagraphs(enabled: boolean) {
    this.dimInactiveParagraphs = enabled;
    try {
      localStorage.setItem('jotter.dimInactiveParagraphs', String(enabled));
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
      localStorage.setItem('jotter.editorFontSize', String(next));
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
      localStorage.setItem('jotter.accentMode', this.accentColorMode);
      localStorage.setItem('jotter.accentPreset', this.accentPreset);
      localStorage.setItem('jotter.customAccentColor', this.customAccentColor);
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
      localStorage.setItem('jotter.showHiddenFiles', String(show));
    } catch {
      // ignore
    }
  }

  setImageLinkFormat(fmt: 'wikilink' | 'markdown') {
    this.imageLinkFormat = fmt;
    try {
      localStorage.setItem('jotter.imageLinkFormat', fmt);
    } catch {
      // ignore
    }
  }

  /** Font for note text; titles stay in the display serif. Applied through `data-font` on the root. */
  setWritingFont(font: WritingFont) {
    this.writingFont = font;
    this.applyWritingFontToDocument();
    try {
      localStorage.setItem('jotter.writingFont', font);
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

  setSummon(enabled: boolean, shortcut = this.summonShortcut) {
    this.summonEnabled = enabled;
    this.summonShortcut = shortcut.trim() || defaultSummonShortcut;
    this.persist('jotter.summonEnabled', String(enabled));
    this.persist('jotter.summonShortcut', this.summonShortcut);
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
      localStorage.setItem(key, value);
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
        localStorage.setItem(
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
        localStorage.setItem(
          'jotter.sidebarCollapsed',
          String(this.sidebarCollapsed)
        );
        localStorage.setItem(
          'jotter.showRightPanel',
          String(this.showRightPanel)
        );
        localStorage.removeItem('jotter.preFocusLayout');
      } catch {
        // ignore
      }
    }

    this.focusMode = next;
    this.topHoverReveal = false;
    this.showSettings = false;
    try {
      localStorage.setItem('jotter.focusMode', next);
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
      localStorage.setItem('jotter.themePref', next);
    } catch {
      // ignore
    }
    this.applyThemeToDocument(next);
  }

  setSidebarWidth(width: number) {
    this.sidebarWidth = width;
    try {
      localStorage.setItem('jotter.sidebarWidth', String(width));
    } catch {
      // ignore
    }
  }

  toggleSidebar() {
    this.sidebarCollapsed = !this.sidebarCollapsed;
    try {
      localStorage.setItem(
        'jotter.sidebarCollapsed',
        String(this.sidebarCollapsed)
      );
    } catch {
      // ignore
    }
  }

  setLeftSidebarView(view: LeftSidebarView) {
    if (this.focusMode !== 'off') {
      this.exitFocusMode();
    }
    this.leftSidebarView = view;
    if (this.sidebarCollapsed) {
      this.sidebarCollapsed = false;
      try {
        localStorage.setItem('jotter.sidebarCollapsed', 'false');
      } catch {
        // ignore
      }
    }
    try {
      localStorage.setItem('jotter.leftSidebarView', view);
    } catch {
      // ignore
    }
  }

  toggleRightPanel() {
    this.showRightPanel = !this.showRightPanel;
    try {
      localStorage.setItem(
        'jotter.showRightPanel',
        String(this.showRightPanel)
      );
    } catch {
      // ignore
    }
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
      const savedMode = localStorage.getItem('jotter.copyMode') as CopyMode | null;
      if (
        savedMode === 'markdown_source' ||
        savedMode === 'plain' ||
        savedMode === 'rich'
      ) {
        this.copyMode = savedMode;
      }
      const savedTheme = localStorage.getItem(
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
      const savedWidth = Number(localStorage.getItem('jotter.sidebarWidth'));
      if (savedWidth >= 180 && savedWidth <= 480) {
        this.sidebarWidth = savedWidth;
      }
      const savedCollapsed = localStorage.getItem('jotter.sidebarCollapsed');
      if (savedCollapsed === 'true' || savedCollapsed === 'false') {
        this.sidebarCollapsed = savedCollapsed === 'true';
      }
      const savedLeftView = localStorage.getItem(
        'jotter.leftSidebarView'
      ) as LeftSidebarView | null;
      if (
        savedLeftView === 'files' ||
        savedLeftView === 'search' ||
        savedLeftView === 'tags' ||
        savedLeftView === 'calendar' ||
        savedLeftView === 'trash'
      ) {
        this.leftSidebarView = savedLeftView;
      }
      const savedRightPanel = localStorage.getItem('jotter.showRightPanel');
      if (savedRightPanel === 'true' || savedRightPanel === 'false') {
        this.showRightPanel = savedRightPanel === 'true';
      }
      const savedEditingMode = localStorage.getItem(
        'jotter.editingMode'
      ) as EditingMode | null;
      if (savedEditingMode === 'live' || savedEditingMode === 'source') {
        this.editingMode = savedEditingMode;
      }
      const savedTypewriter = localStorage.getItem('jotter.typewriterScrolling');
      if (savedTypewriter === 'true' || savedTypewriter === 'false') {
        this.typewriterScrolling = savedTypewriter === 'true';
      }
      const savedDim = localStorage.getItem('jotter.dimInactiveParagraphs');
      if (savedDim === 'true' || savedDim === 'false') {
        this.dimInactiveParagraphs = savedDim === 'true';
      }
      const savedFontSize = Number(localStorage.getItem('jotter.editorFontSize'));
      if ([12, 14, 16, 20].includes(savedFontSize)) {
        this.editorFontSize = savedFontSize;
      }
      document.documentElement.style.setProperty(
        '--editor-font-size',
        `${this.editorFontSize}px`
      );
      const savedAccentMode = localStorage.getItem('jotter.accentMode');
      if (savedAccentMode === 'preset' || savedAccentMode === 'system' || savedAccentMode === 'custom') {
        this.accentColorMode = savedAccentMode;
      } else if (localStorage.getItem('jotter.accentColorMode') === 'custom') {
        // Keep a custom accent chosen before presets existed. The old 'system' default becomes Ink.
        this.accentColorMode = 'custom';
      }
      const savedPreset = localStorage.getItem('jotter.accentPreset');
      if (isAccentPresetId(savedPreset)) {
        this.accentPreset = savedPreset;
      }
      const savedAccentHex = localStorage.getItem('jotter.customAccentColor');
      if (savedAccentHex && /^#[0-9a-fA-F]{6}$/.test(savedAccentHex)) {
        this.customAccentColor = savedAccentHex;
      }
      this.applyAccentToDocument();
      const savedHidden = localStorage.getItem('jotter.showHiddenFiles');
      if (savedHidden === 'true' || savedHidden === 'false') {
        this.showHiddenFiles = savedHidden === 'true';
      }
      const savedImgFormat = localStorage.getItem('jotter.imageLinkFormat');
      if (savedImgFormat === 'wikilink' || savedImgFormat === 'markdown') {
        this.imageLinkFormat = savedImgFormat;
      }
      const savedFont = localStorage.getItem('jotter.writingFont');
      if (savedFont === 'serif' || savedFont === 'sans' || savedFont === 'mono') {
        this.writingFont = savedFont;
      }
      this.applyWritingFontToDocument();
      const savedNav = localStorage.getItem('jotter.notesNavigation');
      if (savedNav === 'folders' || savedNav === 'tags') this.notesNavigation = savedNav;
      const savedHome = localStorage.getItem('jotter.showHomePage');
      if (savedHome === 'true' || savedHome === 'false') this.showHomePage = savedHome === 'true';
      this.showLineNumbers = localStorage.getItem('jotter.showLineNumbers') === 'true';
      const savedSpell = localStorage.getItem('jotter.spellCheck');
      if (savedSpell === 'true' || savedSpell === 'false') {
        this.spellCheck = savedSpell === 'true';
      }
      try {
        const savedSnippets = JSON.parse(localStorage.getItem('jotter.enabledSnippets') ?? '[]');
        if (Array.isArray(savedSnippets)) {
          this.enabledSnippets = savedSnippets.filter((n) => typeof n === 'string');
        }
      } catch {
        this.enabledSnippets = [];
      }
      const savedDailyFolder = localStorage.getItem('jotter.dailyFolder');
      if (savedDailyFolder !== null) this.dailyFolder = savedDailyFolder;
      const savedDailyTemplate = localStorage.getItem('jotter.dailyTemplatePath');
      if (savedDailyTemplate !== null) this.dailyTemplatePath = savedDailyTemplate;
      this.summonEnabled = localStorage.getItem('jotter.summonEnabled') === 'true';
      const savedShortcut = localStorage.getItem('jotter.summonShortcut');
      if (savedShortcut) this.summonShortcut = savedShortcut;
      // The summon hotkey used to force tray residency and hide-on-close. Keep that behavior
      // for anyone who had it on before the two settings became independent.
      const savedTray = localStorage.getItem('jotter.trayEnabled');
      const savedCloseAction = localStorage.getItem('jotter.closeAction');
      if (savedTray === null && savedCloseAction === null) {
        this.trayEnabled = this.summonEnabled;
        this.closeAction = this.summonEnabled ? 'tray' : 'quit';
      } else {
        this.trayEnabled = savedTray === 'true';
        if (savedCloseAction === 'tray' && this.trayEnabled) this.closeAction = 'tray';
      }
      const savedPreFocus = localStorage.getItem('jotter.preFocusLayout');
      if (savedPreFocus) {
        try {
          this.preFocusLayout = JSON.parse(savedPreFocus) as SavedPreFocusLayout;
        } catch {
          this.preFocusLayout = null;
        }
      }
      const savedFocusMode = localStorage.getItem(
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

    return () => {
      mediaQuery.removeEventListener('change', handleMediaChange);
    };
  }
}

export const settingsStore = new SettingsStore();
