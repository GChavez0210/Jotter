import { aiCancel, aiDeleteKey, aiGetStatus, aiRun, aiSaveConfig, aiSaveKey } from '../ipc';
import type { AiConfig, AiOperation, AiProvider, AiResult, AiStatus, RewriteStyle } from '../types';

export const AI_PROVIDER_LABELS: Record<AiProvider, string> = {
  'openai-compatible': 'OpenAI-compatible',
  anthropic: 'Anthropic',
  ollama: 'Ollama',
};

export const REWRITE_STYLE_LABELS: Record<RewriteStyle, string> = {
  polish: 'Polish',
  shorter: 'Shorten',
  formal: 'More formal',
  casual: 'More casual',
  grammar: 'Fix grammar',
};

export const REWRITE_ACTIONS = ['polish', 'shorter', 'grammar'] as const;
export const REWRITE_TONES = ['formal', 'casual'] as const;

/** Preserve saved choices when the redundant Clearer command becomes Polish. */
export function normalizeRewriteStyle(value: string | null): RewriteStyle {
  if (value === 'clearer') return 'polish';
  return value && Object.hasOwn(REWRITE_STYLE_LABELS, value) ? value as RewriteStyle : 'polish';
}

export type AiOperationKind = AiOperation['kind'];

const LANGUAGE_KEY = 'jotter.aiTargetLanguage';
const STYLE_KEY = 'jotter.aiRewriteStyle';

export type AiRun =
  | { phase: 'idle' }
  | { phase: 'running'; operationId: string }
  | { phase: 'done'; result: AiResult }
  | { phase: 'error'; message: string };

function endpointKey(provider: AiProvider): 'openaiCompatible' | 'anthropic' | 'ollama' {
  return provider === 'openai-compatible' ? 'openaiCompatible' : provider;
}

/**
 * Bring-your-own AI state. Settings and keys live in Rust; this store holds a copy of the
 * settings and each command tab's request state while the AI panel is open.
 */
class AiStore {
  status = $state<AiStatus | null>(null);
  loadError = $state('');
  showPanel = $state(false);
  panelOperation = $state<AiOperationKind>('rewrite');
  targetLanguage = $state('Spanish');
  rewriteStyle = $state<RewriteStyle>('polish');
  private runs = $state<Record<AiOperationKind, AiRun>>({
    rewrite: { phase: 'idle' }, translate: { phase: 'idle' }, summarize: { phase: 'idle' },
  });

  get run(): AiRun {
    return this.runs[this.panelOperation];
  }

  set run(value: AiRun) {
    this.runs[this.panelOperation] = value;
  }
  private loading: Promise<void> | null = null;
  private settingsQueue: Promise<void> = Promise.resolve();

  private queueSettings(task: () => Promise<void>): Promise<void> {
    const pending = this.settingsQueue.then(task);
    this.settingsQueue = pending.catch(() => {});
    return pending;
  }

  constructor() {
    try {
      const lang = localStorage.getItem(LANGUAGE_KEY);
      if (lang) this.targetLanguage = lang;
      this.rewriteStyle = normalizeRewriteStyle(localStorage.getItem(STYLE_KEY));
    } catch {
      // Storage unavailable; keep defaults.
    }
  }

  enabled = $derived(this.status?.config.enabled ?? false);

  ensureLoaded(): Promise<void> {
    this.loading ??= this.reload();
    return this.loading;
  }

  async reload() {
    return this.queueSettings(async () => {
      try {
        this.status = await aiGetStatus();
        this.loadError = '';
      } catch (err) {
        this.loadError = String(err);
      }
    });
  }

  endpoint(provider: AiProvider) {
    return this.status?.config[endpointKey(provider)];
  }

  keyStatus(provider: AiProvider) {
    return this.status?.keys.find((k) => k.provider === provider);
  }

  /** Saves a changed copy of the config. Rejects with Rust's validation message. */
  async updateConfig(change: (config: AiConfig) => void) {
    if (!this.status) await this.ensureLoaded();
    return this.queueSettings(async () => {
      if (!this.status) throw new Error(this.loadError || 'AI settings are unavailable.');
      const next: AiConfig = $state.snapshot(this.status.config);
      change(next);
      this.status = await aiSaveConfig(next);
    });
  }

  setEndpointField(provider: AiProvider, field: 'baseUrl' | 'model', value: string) {
    return this.updateConfig((c) => {
      c[endpointKey(provider)][field] = value;
    });
  }

  async saveKey(provider: AiProvider, key: string) {
    return this.queueSettings(async () => {
      this.status = await aiSaveKey(provider, key);
    });
  }

  async deleteKey(provider: AiProvider) {
    return this.queueSettings(async () => {
      this.status = await aiDeleteKey(provider);
    });
  }

  setTargetLanguage(lang: string) {
    this.targetLanguage = lang;
    this.persist(LANGUAGE_KEY, lang);
  }

  setRewriteStyle(style: RewriteStyle) {
    this.rewriteStyle = style;
    this.persist(STYLE_KEY, style);
  }

  openPanel(kind: AiOperationKind) {
    this.panelOperation = kind;
    this.showPanel = true;
    void this.ensureLoaded();
  }

  closePanel() {
    for (const kind of ['rewrite', 'translate', 'summarize'] as const) {
      this.cancel(kind);
      this.runs[kind] = { phase: 'idle' };
    }
    this.showPanel = false;
  }

  operation(): AiOperation {
    switch (this.panelOperation) {
      case 'summarize':
        return { kind: 'summarize' };
      case 'rewrite':
        return { kind: 'rewrite', style: this.rewriteStyle };
      case 'translate':
        return { kind: 'translate', targetLanguage: this.targetLanguage.trim() };
    }
  }

  /** Runs `operation` on `text`. A cancelled or superseded request never changes `run`. */
  async start(operation: AiOperation, text: string) {
    const kind = operation.kind;
    this.cancel(kind);
    this.panelOperation = kind;
    const operationId = crypto.randomUUID();
    this.runs[kind] = { phase: 'running', operationId };
    try {
      const result = await aiRun({ operationId, operation, text });
      if (this.isRunning(kind, operationId)) this.runs[kind] = { phase: 'done', result };
    } catch (err) {
      if (this.isRunning(kind, operationId)) this.runs[kind] = { phase: 'error', message: String(err) };
    }
  }

  cancel(kind: AiOperationKind = this.panelOperation) {
    const run = this.runs[kind];
    if (run.phase !== 'running') return;
    const { operationId } = run;
    this.runs[kind] = { phase: 'idle' };
    void aiCancel(operationId).catch(() => {});
  }

  private isRunning(kind: AiOperationKind, operationId: string) {
    const run = this.runs[kind];
    return run.phase === 'running' && run.operationId === operationId;
  }

  private persist(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // ignore storage errors
    }
  }
}

export const aiStore = new AiStore();
