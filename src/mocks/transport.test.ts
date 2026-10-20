import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

describe("M6 MVP transport", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.stubEnv("VITE_MOCK", "0");
  });
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  it("sends every API.md §3 command to Rust when mocks are off", async () => {
    const api = await import("../api");
    const invoke = vi.fn(async <T>(_cmd: string, _args?: Record<string, unknown>) => undefined as T) as unknown as Parameters<typeof api.__setTransportForTests>[0]["invoke"];
    const listen = vi.fn(async () => () => undefined);
    api.__setTransportForTests({ invoke, listen });
    const mockCall = vi.spyOn(await import("../mocks"), "mockCall");
    const calls = [
      api.getCounts(), api.getDashboard(), api.listItems({}), api.getItem(1),
      api.createItem({ type: "album", display_name: "x", spotify_uri: null, provenance: "user" }),
      api.updateItem(1, {}), api.setStatus(1, "planned"), api.deleteItem(1),
      api.linkItems(1, 2, "by_artist"), api.unlinkItems(1, 2, "by_artist"), api.reorderBacklog([1]),
      api.parseLink("spotify:album:AAAAAAAAAAAAAAAAAAAAAA"), api.resolveLink("https://spotify.link/x"),
      api.prefillTitle("spotify:album:AAAAAAAAAAAAAAAAAAAAAA"), api.listUnresolved(), api.retryUnresolved(),
      api.discardUnresolved(1), api.search("x"), api.listTags(), api.renameTag(1, "x"),
      api.mergeTags(1, 2), api.deleteTag(1), api.listSavedFilters(), api.saveFilter("x", {}),
      api.deleteSavedFilter(1), api.getRediscover(), api.getOnline(), api.getSettings(), api.updateSettings({}),
      api.getCapture(), api.setCaptureEnabled(false), api.newPairingCode(), api.exportLibrary(),
      api.listBackups(), api.backupNow(), api.pickBackup(), api.checkBackup("x.zip"), api.restoreBackup("x.zip"),
      api.chooseBackupDir(), api.moveLibrary(), api.deleteLibrary("DELETE"), api.recordPageVisit("home"),
      api.getSuccessMetrics(),
    ];
    await Promise.all(calls);
    expect(invoke).toHaveBeenCalledTimes(calls.length);
    expect(mockCall).not.toHaveBeenCalled();
  });

  it("uses Rust for MVP events while leaving the R2 logger event mocked", async () => {
    const api = await import("../api");
    const invoke = vi.fn(async <T>(_cmd: string, _args?: Record<string, unknown>) => undefined as T) as unknown as Parameters<typeof api.__setTransportForTests>[0]["invoke"];
    const listen = vi.fn(async () => () => undefined);
    api.__setTransportForTests({ invoke, listen });
    const mockListen = vi.spyOn(await import("../mocks"), "mockListen");
    const offCapture = api.onEvent("capture-received", () => undefined);
    const offBackup = api.onEvent("backup-finished", () => undefined);
    const offLogger = api.onEvent("logger-status", () => undefined);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(listen).toHaveBeenCalledTimes(2);
    expect(mockListen).toHaveBeenCalledTimes(1);
    offCapture();
    offBackup();
    offLogger();
  });
});
