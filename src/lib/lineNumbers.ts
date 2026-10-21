import { RangeSetBuilder } from '@codemirror/state';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from '@codemirror/view';

/**
 * Line numbers drawn inside each line rather than in a gutter. A gutter row starts at the top
 * of the line block, so headings, code blocks, and tables with top padding would show their
 * number above the text. A number placed in the line inherits its font and line height, so it
 * shares the first row's baseline, and its absolute position keeps it out of the text flow.
 */
class LineNumberWidget extends WidgetType {
  constructor(readonly number: number) {
    super();
  }

  eq(other: LineNumberWidget): boolean {
    return other.number === this.number;
  }

  toDOM(): HTMLElement {
    const outer = document.createElement('span');
    outer.className = 'cm-line-number';
    outer.setAttribute('aria-hidden', 'true');
    const digits = document.createElement('span');
    digits.textContent = String(this.number);
    outer.append(digits);
    return outer;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

function buildLineNumbers(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  let lastLine = 0;
  for (const { from, to } of view.visibleRanges) {
    let pos = from;
    while (pos <= to) {
      const line = view.state.doc.lineAt(pos);
      if (line.number > lastLine) {
        builder.add(
          line.from,
          line.from,
          Decoration.widget({ widget: new LineNumberWidget(line.number), side: -1 })
        );
        lastLine = line.number;
      }
      pos = line.to + 1;
    }
  }
  return builder.finish();
}

export function createLineNumbersExtension() {
  return [
    ViewPlugin.fromClass(
      class {
        decorations: DecorationSet;

        constructor(view: EditorView) {
          this.decorations = buildLineNumbers(view);
        }

        update(update: ViewUpdate) {
          if (update.docChanged || update.viewportChanged || update.heightChanged) {
            this.decorations = buildLineNumbers(update.view);
          }
        }
      },
      { decorations: (plugin) => plugin.decorations }
    ),
    EditorView.editorAttributes.of({ class: 'cm-with-line-numbers' }),
  ];
}
