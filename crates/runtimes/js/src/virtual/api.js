const unsupported = (name) => () => {
  throw new Error(`spiders-wm/api runtime stub does not implement ${name}`);
};

export const events = {
  on: unsupported("events.on"),
  once: unsupported("events.once"),
  off: unsupported("events.off"),
};

export const wm = {
  spawn: unsupported("wm.spawn"),
  reloadConfig: unsupported("wm.reloadConfig"),
  setLayout: unsupported("wm.setLayout"),
  cycleLayout: unsupported("wm.cycleLayout"),
  viewWorkspace: unsupported("wm.viewWorkspace"),
  toggleViewWorkspace: unsupported("wm.toggleViewWorkspace"),
  toggleFloating: unsupported("wm.toggleFloating"),
  toggleFullscreen: unsupported("wm.toggleFullscreen"),
  focusDirection: unsupported("wm.focusDirection"),
  closeWindow: unsupported("wm.closeWindow"),
};

export const query = {
  getState: unsupported("query.getState"),
  getFocusedWindow: unsupported("query.getFocusedWindow"),
  getCurrentMonitor: unsupported("query.getCurrentMonitor"),
  getCurrentWorkspace: unsupported("query.getCurrentWorkspace"),
};
