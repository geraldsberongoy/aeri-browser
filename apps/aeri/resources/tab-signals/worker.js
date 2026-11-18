// Aeri's tab signals (ADR 0007). Chrome knows which tabs are audible; CEF
// does not say. Aeri attaches to this worker over its private DevTools pipe
// and adds the `aeriTabSignals` binding; every report is the whole set of
// Chrome window ids with an audible tab (one Aeri tab per Chrome window), so
// a lost report is repaired by the next one.

self.report = async () => {
  const tabs = await chrome.tabs.query({});
  const audible = tabs.filter((tab) => tab.audible).map((tab) => tab.windowId);
  if (typeof self.aeriTabSignals === "function") {
    self.aeriTabSignals(JSON.stringify({ audible }));
  }
};

chrome.tabs.onUpdated.addListener((_id, change) => {
  if ("audible" in change) self.report();
});
chrome.tabs.onRemoved.addListener(() => self.report());
