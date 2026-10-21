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
    expect(vi.mocked(invoke).mock.calls.map(([command]) => command)).toEqual([
      "get_counts", "get_dashboard", "list_items", "get_item", "create_item", "update_item", "set_status",
      "delete_item", "link_items", "unlink_items", "reorder_backlog", "parse_link", "resolve_link", "prefill_title",
      "list_unresolved", "retry_unresolved", "discard_unresolved", "search", "list_tags", "rename_tag", "merge_tags",
      "delete_tag", "list_saved_filters", "save_filter", "delete_saved_filter", "get_rediscover", "get_online",
      "get_settings", "update_settings", "get_capture", "set_capture_enabled", "new_pairing_code", "export_library",
      "list_backups", "backup_now", "pick_backup", "check_backup", "restore_backup", "choose_backup_dir",
      "move_library", "delete_library", "record_page_visit", "get_success_metrics",
    ]);
    // R2/R3 aren't on Rust yet and a real build carries no mocks: they fail instead of showing mock data.
    await expect(api.getLoggerStatus()).rejects.toThrow(/Release 2\/3/);
    await expect(api.getSpotifyStatus()).rejects.toThrow(/Release 2\/3/);
    expect(invoke).toHaveBeenCalledTimes(calls.length);
    expect(mockCall).not.toHaveBeenCalled();
  });

  it("uses Rust for MVP events and never loads mocks for the R2 logger event", async () => {
    const api = await import("../api");
    const invoke = vi.fn(async <T>(_cmd: string, _args?: Record<string, unknown>) => undefined as T) as unknown as Parameters<typeof api.__setTransportForTests>[0]["invoke"];
    const listen = vi.fn(async () => () => undefined);
    api.__setTransportForTests({ invoke, listen });
    const mockListen = vi.spyOn(await import("../mocks"), "mockListen");
    const offOnline = api.onEvent("online-changed", () => undefined);
    const offCapture = api.onEvent("capture-received", () => undefined);
    const offDisabled = api.onEvent("capture-disabled", () => undefined);
    const offBackup = api.onEvent("backup-finished", () => undefined);
    const offResolved = api.onEvent("unresolved-resolved", () => undefined);
    const offLogger = api.onEvent("logger-status", () => undefined);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(listen).toHaveBeenCalledTimes(5);
    expect(mockListen).not.toHaveBeenCalled();
    offOnline();
    offCapture();
    offDisabled();
    offBackup();
    offResolved();
    offLogger();
  });

  it("hides R2/R3 screens and keeps the real clock when mocks are off", async () => {
    const { renderHook } = await import("@testing-library/react");
    const { useRelease } = await import("../hooks/useRelease");
    expect(renderHook(() => useRelease()).result.current).toEqual({ release: "mvp", r2: false, r3: false });
    const before = Date.now();
    await import("../mocks");
    const { today } = await import("../lib/format");
    expect(today().getTime()).toBeGreaterThanOrEqual(before);
  }, 30_000); // fresh imports of Testing Library and the mock dataset after resetModules: ~7 s under a loaded check.py

  it("a phone share or resolved link from Rust makes readers refetch", async () => {
    const api = await import("../api");
    const handlers = new Map<string, (e: { payload: unknown }) => void>();
    const listen = vi.fn(async (name: string, h: (e: { payload: unknown }) => void) => { handlers.set(name, h); return () => undefined; });
    api.__setTransportForTests({ invoke: vi.fn() as never, listen: listen as never });
    const refetch = vi.fn();
    api.onInvalidate(refetch);
    const off = api.invalidateOnDataEvents();
    await new Promise((resolve) => setTimeout(resolve, 0));
    for (const e of ["capture-received", "capture-disabled", "unresolved-resolved", "backup-finished"]) handlers.get(e)!({ payload: {} });
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(refetch).toHaveBeenCalledTimes(1); // one burst, one refetch
    off();
  });
});
