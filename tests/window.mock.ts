export const windowControl = { sizes: [] as Array<{width: number; height: number}> };
export class LogicalSize {
  constructor(public width: number, public height: number) {}
}
export const getCurrentWindow = () => ({
  setSize: async (size: LogicalSize) => { windowControl.sizes.push(size); },
  setFullscreen: async (_fullscreen: boolean) => {},
});
