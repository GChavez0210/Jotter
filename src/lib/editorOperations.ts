import type { EditorView } from '@codemirror/view';

/** Cancel delayed clipboard mutations if their document changed or was replaced. */
export function captureEditorOperation(view: EditorView, generation: number) {
  const doc = view.state.doc;
  const selection = view.state.selection.main;
  return {
    from: selection.from,
    to: selection.to,
    isCurrent(current: EditorView, currentGeneration: number) {
      return generation === currentGeneration && current.state.doc === doc;
    },
  };
}
