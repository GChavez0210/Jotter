import { getCurrentWindow } from '@tauri-apps/api/window';
import { designTokens } from '../designTokens';
import { getBackdropState, syncWindowTheme } from '../ipc';
import type {
  BackdropState,
  CopyMode,
  EditingMode,
  FocusMode,
  ThemePreference,
} from '../types';

export const COPY_MODE_LABELS: Record<CopyMode, { label: string; desc: string }> = {
  markdown_source: {
    label: 'Markdown source',
    desc: 'Default: raw Markdown with <br> & &nbsp; HTML plus plain text',
  },
  plain: {
    label: 'Plain',
    desc: 'Opt-in clean text: text/plain only (arboard set_text, Markdown stripped)',
  },
  rich: {
    label: 'Rich',
    desc: 'HTML (<br> newlines, &nbsp; indent, formatting) plus plain text',
  },
};

interface SavedPreFocusLayout {
  sidebarCollapsed: boolean;
  showInspector: boolean;
}

class SettingsStore {
  copyMode = $state<CopyMode>('markdown_source');
  themePref = $state<ThemePreference>('system');
  systemIsDark = $state(false);
  forceSolidBackdrop = $state(false);
  backdropState = $state<BackdropState>({
    micaApplied: true,
    backdropKind: 'mica',
    darkMode: null,
  });

  // Milestone 3: Editing Mode, Focus Mode & Focus Options
  editingMode = $state<EditingMode>('live');
  focusMode = $state<FocusMode>('off');
  typewriterScrolling = $state(false);
  dimInactiveParagraphs = $state(false);
  showCommandPalette = $state(false);
  topHoverReveal = $state(false);

  sidebarWidth = $state<number>(designTokens.layout.sidebarDefaultWidth);
  sidebarCollapsed = $state(false);
  showSettings = $state(false);
  showInspector = $state(false);

  private preFocusLayout: SavedPreFocusLayout | null = null;

  effectiveDark = $derived(
    this.themePref === 'system' ? this.systemIsDark : this.themePref === 'dark'
  );

  effectiveSolidBackdrop = $derived(
    this.forceSolidBackdrop || !this.backdropState.micaApplied
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

  setFocusMode(next: FocusMode) {
    if (this.focusMode === 'off' && next !== 'off') {
      this.preFocusLayout = {
        sidebarCollapsed: this.sidebarCollapsed,
        showInspector: this.showInspector,
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
      this.preFocusLayout = null;
      try {
        localStorage.setItem(
          'jotter.sidebarCollapsed',
          String(this.sidebarCollapsed)
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
    this.applyThemeToDocument(next, this.systemIsDark);
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

  applyThemeToDocument(pref: ThemePreference, sysDark: boolean) {
    const root = document.documentElement;
    if (pref === 'system') {
      root.removeAttribute('data-theme');
    } else {
      root.setAttribute('data-theme', pref);
    }
    const isDark = pref === 'system' ? sysDark : pref === 'dark';
    void syncWindowTheme(isDark)
      .then((res) => {
        this.backdropState = res;
      })
      .catch(() => {
        this.backdropState = {
          micaApplied: false,
          backdropKind: 'solid_fallback',
          darkMode: isDark,
        };
      });
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
        savedTheme === 'dark'
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
    this.applyThemeToDocument(this.themePref, this.systemIsDark);

    const handleMediaChange = (e: MediaQueryListEvent) => {
      this.systemIsDark = e.matches;
      this.applyThemeToDocument(this.themePref, this.systemIsDark);
    };
    mediaQuery.addEventListener('change', handleMediaChange);

    void getBackdropState()
      .then((s) => {
        this.backdropState = s;
      })
      .catch(() => {
        this.backdropState = {
          micaApplied: false,
          backdropKind: 'solid_fallback',
          darkMode: this.systemIsDark,
        };
      });

    return () => {
      mediaQuery.removeEventListener('change', handleMediaChange);
    };
  }
}

export const settingsStore = new SettingsStore();
