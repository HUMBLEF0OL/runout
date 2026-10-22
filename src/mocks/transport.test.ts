import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

describe("Rust transport (VITE_MOCK=0)", () => {
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
    expect(mockCall).not.toHaveBeenCalled();
  });

  it("sends every API.md §4 (R2) and §5 (R3) command to Rust with the Rust parameter names", async () => {
    const api = await import("../api");
    const invoke = vi.fn(async <T>(_cmd: string, _args?: Record<string, unknown>) => undefined as T) as unknown as Parameters<typeof api.__setTransportForTests>[0]["invoke"];
    api.__setTransportForTests({ invoke, listen: vi.fn(async () => () => undefined) });
    const mockCall = vi.spyOn(await import("../mocks"), "mockCall");
    const P = "spotify:playlist:AAAAAAAAAAAAAAAAAAAAAA";
    const draft = { playlist_uri: P, snapshot_id: "s", tracks: [] };
    await Promise.all([
      api.getLoggerStatus(), api.setLogger({ on: true }), api.pauseLogger(60), api.resumeLogger(),
      api.listPlays({ tab: "all", range: "7d", offset: 0, limit: 50 }), api.listUnmatched(), api.linkPlay(1, 2),
      api.getItemListening(1), api.pickImportSource(), api.previewImport("x.zip"), api.runImport("x.zip"),
      api.listImports(), api.getFreshness(), api.getListeningInsights("12m"), api.getCollectionInsights("12m"),
      api.getTakeInsights(), api.getYearReview(2026), api.listExportedPlaylists(), api.getPlaylistCheckup(1),
      api.getSpotifyStatus(), api.connectSpotify("Abc123"), api.disconnectSpotify(), api.listSpotifyPlaylists(),
      api.loadStudioDraft(P), api.previewStudioApply(draft, []), api.applyStudioDraft(draft, []), api.getLastApply(P),
      api.undoLastApply(P), api.createPlaylistFromFilter(1, "n", false), api.createPlaylistFromItems([1], "n", true),
      api.autofill("spotify:album:AAAAAAAAAAAAAAAAAAAAAA"), api.bulkAddAlbumTracks(1, [{ uri: "spotify:track:AAAAAAAAAAAAAAAAAAAAAA", name: "t" }]),
      api.getTopItems("tracks", "4w"), api.getRecentPlaysApi(), api.libraryContains([]), api.saveToSpotifyLibrary([]),
      api.removeFromSpotifyLibrary([]), api.findCrossPlaylistDupes(P), api.getInbox(), api.createInbox(), api.checkInbox(),
      api.setInboxOptions(true, false),
    ]);
    expect(mockCall).not.toHaveBeenCalled();
    const sent = Object.fromEntries(vi.mocked(invoke).mock.calls.map(([cmd, args]) => [cmd, Object.keys(args ?? {}).sort()]));
    // Argument keys = the commands.rs parameters, camelCased as Tauri expects (check.py "commands" checks the same).
    expect(sent).toEqual({
      get_logger_status: [], set_logger: ["patch"], pause_logger: ["minutes"], resume_logger: [], list_plays: ["q"],
      list_unmatched: [], link_play: ["eventId", "itemId"], get_item_listening: ["id"], pick_import_source: [],
      preview_import: ["path"], run_import: ["path"], list_imports: [], get_freshness: [], get_listening_insights: ["range"],
      get_collection_insights: ["range"], get_take_insights: [], get_year_review: ["year"], list_exported_playlists: [],
      get_playlist_checkup: ["id"], get_spotify_status: [], connect_spotify: ["clientId"], disconnect_spotify: [],
      list_spotify_playlists: [], load_studio_draft: ["playlistUri"], preview_studio_apply: ["draft", "keep"],
      apply_studio_draft: ["draft", "keep"], get_last_apply: ["playlistUri"], undo_last_apply: ["playlistUri"],
      create_playlist_from_filter: ["filterId", "name", "public"], create_playlist_from_items: ["itemIds", "name", "public"],
      autofill: ["uri"], bulk_add_album_tracks: ["albumId", "tracks"], get_top_items: ["kind", "range"],
      get_recent_plays_api: [], library_contains: ["uris"], save_to_spotify_library: ["uris"], remove_from_spotify_library: ["uris"],
      find_cross_playlist_dupes: ["playlistUri"], get_inbox: [], create_inbox: [], check_inbox: [], set_inbox_options: ["followedArtists", "savedAlbums"],
    });
  });

  it("the rail's freshness box shows the dates get_freshness returns", async () => {
    const api = await import("../api");
    const invoke = vi.fn(async () => ({ logger_last_event: "2026-10-20T12:00:00Z", export_latest_event: "2026-10-12T12:00:00Z" }));
    api.__setTransportForTests({ invoke: invoke as never, listen: vi.fn(async () => () => undefined) });
    const { freshnessText } = await import("../components/logic");
    expect(freshnessText(await api.getFreshness(), "listening")).toBe("Desktop plays: live. All devices: up to your export of 12 Oct.");
    expect(invoke).toHaveBeenCalledWith("get_freshness", {});
  });

  it("uses Rust for every API.md §6 event and never loads the mocks", async () => {
    const api = await import("../api");
    const listen = vi.fn(async (_name: string, _h: unknown) => () => undefined);
    api.__setTransportForTests({ invoke: vi.fn() as never, listen: listen as never });
    const mockListen = vi.spyOn(await import("../mocks"), "mockListen");
    const names = ["online-changed", "capture-received", "capture-disabled", "backup-finished", "unresolved-resolved",
      "logger-status", "play-logged", "spotify-status", "inbox-imported", "open-quick-add"] as const;
    const offs = names.map((n) => api.onEvent(n, () => undefined));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(vi.mocked(listen).mock.calls.map(([n]) => n)).toEqual([...names]);
    expect(mockListen).not.toHaveBeenCalled();
    offs.forEach((off) => off());
  });

  it("shows every release's screens and keeps the real clock when mocks are off", async () => {
    const { renderHook } = await import("@testing-library/react");
    const { useRelease } = await import("../hooks/useRelease");
    expect(renderHook(() => useRelease()).result.current).toEqual({ release: "r3", r2: true, r3: true });
    const before = Date.now();
    await import("../mocks");
    const { today } = await import("../lib/format");
    expect(today().getTime()).toBeGreaterThanOrEqual(before);
  }, 30_000); // fresh imports of Testing Library and the mock dataset after resetModules: ~7 s under a loaded check.py

  it("a phone share, resolved link, logged play or logger/Spotify state change from Rust makes readers refetch", async () => {
    const api = await import("../api");
    const handlers = new Map<string, (e: { payload: unknown }) => void>();
    const listen = vi.fn(async (name: string, h: (e: { payload: unknown }) => void) => { handlers.set(name, h); return () => undefined; });
    api.__setTransportForTests({ invoke: vi.fn() as never, listen: listen as never });
    const refetch = vi.fn();
    api.onInvalidate(refetch);
    const off = api.invalidateOnDataEvents();
    await new Promise((resolve) => setTimeout(resolve, 0));
    for (const e of ["capture-received", "capture-disabled", "unresolved-resolved", "backup-finished", "play-logged", "logger-status", "spotify-status", "inbox-imported"]) handlers.get(e)!({ payload: {} });
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(refetch).toHaveBeenCalledTimes(1); // one burst, one refetch
    off();
  });
});
