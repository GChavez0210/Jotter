import { designTokens } from '../designTokens';
import { getBackdropState, syncWindowTheme } from '../ipc';
import type { BackdropState, CopyMode, ThemePreference } from '../types';

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

  sidebarWidth = $state<number>(designTokens.layout.sidebarDefaultWidth);
  sidebarCollapsed = $state(false);
  showSettings = $state(false);
  showInspector = $state(false);

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
