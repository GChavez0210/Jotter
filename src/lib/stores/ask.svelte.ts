/** Whether the Ask your vault panel is open. The panel keeps its own request state. */
class AskStore {
  show = $state(false);

  open() {
    this.show = true;
  }

  close() {
    this.show = false;
  }
}

export const askStore = new AskStore();
