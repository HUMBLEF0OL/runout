"""Generate Runout's mock dataset (design prototype now, src/mocks in M2).
Deterministic: same seed -> same data. Run: python scripts/gen_mock_data.py
Writes docs/design/prototype/mock-data.json and mock-data.js (window.RUNOUT_MOCK).
Shapes follow TSD §3. All Spotify IDs are synthetic; no real export values."""
import json
import random
from collections import Counter, defaultdict
from datetime import datetime, timedelta
from pathlib import Path

SEED = 7
TODAY = datetime(2026, 10, 18, 21, 20)
EXPORT_UNTIL = datetime(2026, 10, 2, 23, 59)  # latest export covers plays up to here
OUT = Path(__file__).resolve().parent.parent / "docs/design/prototype"
rnd = random.Random(SEED)
B62 = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"


def sid():
    return "".join(rnd.choice(B62) for _ in range(22))


def iso(d):
    return d.strftime("%Y-%m-%dT%H:%M")


def day(n, h=20, m=0):
    return TODAY.replace(hour=h, minute=m) - timedelta(days=n)


# ---------------------------------------------------------------- artists
ARTISTS = [  # name, tint, affinity (listening weight), status, note
    ("Alice Coltrane", "#3D5A4A", 10, "in_progress", "Working through the Impulse years in order."),
    ("Floating Points", "#2E3F5C", 8, "explored", "Electronic producer; Promises is the way in."),
    ("Pharoah Sanders", "#5C4630", 6, "planned", "Came here from Promises. Start with Thembi."),
    ("Prateek Kuhad", "#4F5B6B", 7, "explored", "Ishaan's pick. Quiet, lyrical, Hindi and English."),
    ("Kate Bush", "#6B3A4A", 5, "explored", ""),
    ("Massive Attack", "#2B4F7A", 5, "in_progress", "Bristol sound. Mezzanine done, Blue Lines next."),
    ("Miles Davis", "#22324A", 4, "explored", ""),
    ("John Coltrane", "#3B3B44", 4, "explored", ""),
    ("Madvillain", "#8A6A2E", 2, "planned", "Ishaan insists. Listen front to back."),
    ("Sufjan Stevens", "#5E6F63", 3, "explored", ""),
    ("LCD Soundsystem", "#585858", 2, "planned", ""),
    ("Air", "#4A5A6B", 2, "explored", ""),
    ("Bill Evans", "#2F3B4A", 3, "explored", ""),
    ("Kelly Lee Owens", "#4A4F59", 2, "planned", ""),
    ("Radiohead", "#3B4A5C", 3, "explored", ""),
    ("Bjork", "#4F3B5C", 2, "explored", ""),
]
# albums: title, artist, tint, status, rating, tags, rec_by, added_days_ago, extra
ALBUMS = [
    ("Journey in Satchidananda", "Alice Coltrane", "#3D5A4A", "explored", 4.5, ["jazz", "spiritual"], "", 120, {"review": "Harp and tanpura doing what most records can't. The title track is the reason to start here."}),
    ("Ptah, the El Daoud", "Alice Coltrane", "#4A5A3B", "planned", None, ["jazz", "spiritual"], "", 6, {"priority": "high"}),
    ("Promises", "Floating Points", "#2E3F5C", "in_progress", 4.5, ["late-night", "jazz", "headphones"], "Meera", 14, {"review": "Movement 6 is where it opens up. Sanders barely plays and it still holds the whole record together.", "favourite": True, "phone": True}),
    ("Thembi", "Pharoah Sanders", "#5C4630", "planned", None, ["jazz", "spiritual"], "Ishaan", 0, {"priority": "medium"}),
    ("cold/mess (single)", "Prateek Kuhad", "#4F5B6B", "explored", 5, ["hindi-indie", "late-night"], "Ishaan", 210, {"review": "Three minutes, no wasted line."}),
    ("Hounds of Love", "Kate Bush", "#6B3A4A", "in_progress", 5, ["art-pop", "headphones"], "", 35, {"favourite": True, "review": "Side two is a suite. Never shuffle it."}),
    ("Mezzanine", "Massive Attack", "#33333A", "explored", 4.5, ["trip-hop", "late-night"], "", 400, {}),
    ("Blue Lines", "Massive Attack", "#2B4F7A", "planned", None, ["trip-hop"], "Meera", 23, {"priority": "medium"}),
    ("Kind of Blue", "Miles Davis", "#22324A", "explored", 5, ["jazz"], "", 900, {"review": "Still the front door to jazz."}),
    ("Bitches Brew", "Miles Davis", "#5C4630", "abandoned", 3, ["jazz", "fusion"], "", 140, {"review": "Stopped at Spanish Key. Try again with time to spare."}),
    ("A Love Supreme", "John Coltrane", "#3B3B44", "explored", 5, ["jazz", "spiritual"], "", 700, {"review": "Part one is a prayer you can hum."}),
    ("Blue Train", "John Coltrane", "#2B4F7A", "explored", 4, ["jazz"], "", 300, {"revisit": True}),
    ("Madvillainy", "Madvillain", "#8A6A2E", "planned", None, ["hip-hop"], "Ishaan", 2, {"priority": "high"}),
    ("Carrie & Lowell", "Sufjan Stevens", "#5E6F63", "explored", 4.5, ["folk", "late-night"], "", 160, {"revisit": True, "review": "Too sad for most days. Winter record."}),
    ("Sound of Silver", "LCD Soundsystem", "#585858", "planned", None, ["dance"], "", 64, {}),
    ("Moon Safari", "Air", "#4A5A6B", "explored", 4, ["electronic"], "", 260, {}),
    ("Waltz for Debby", "Bill Evans", "#2F3B4A", "explored", 4.5, ["jazz", "piano"], "", 330, {"review": "Recorded live; you can hear the glasses clink."}),
    ("Inner Song", "Kelly Lee Owens", "#4A4F59", "planned", None, ["electronic"], "", 9, {}),
    ("In Rainbows", "Radiohead", "#3B4A5C", "explored", 4.5, ["art-rock"], "", 500, {}),
    ("Kid A", "Radiohead", "#3B3B3B", "planned", None, ["art-rock", "electronic"], "", 150, {}),
    ("Homogenic", "Bjork", "#4F3B5C", "explored", 4, ["art-pop"], "", 220, {"revisit": True}),
    ("Elaenia", "Floating Points", "#2E4A5C", "planned", None, ["electronic"], "", 30, {"priority": "low"}),
    ("Universal Consciousness", "Alice Coltrane", "#4A3D5A", "planned", None, ["jazz", "spiritual"], "", 6, {}),
    ("Karam", "Pharoah Sanders", "#5C3D30", "planned", None, ["jazz"], "", 75, {}),
    ("Spirit of Eden", "Talk Talk", "#4A5A50", "planned", None, ["art-rock"], "Meera", 120, {}),
    ("The Koln Concert", "Keith Jarrett", "#5C4630", "planned", None, ["jazz", "piano"], "", 150, {}),
    ("Maggot Brain", "Funkadelic", "#5A3B3B", "planned", None, ["funk"], "", 180, {}),
    ("Pet Sounds", "The Beach Boys", "#5A6B4A", "explored", 4, ["pop"], "", 600, {}),
    ("Remain in Light", "Talking Heads", "#6B5A3B", "explored", 3.5, ["new-wave"], "", 420, {}),
    ("Blackstar", "David Bowie", "#2A2A2E", "explored", None, ["art-rock"], "", 380, {}),
    ("Pink Moon", "Nick Drake", "#5C5A3B", "explored", 4.5, ["folk"], "", 450, {"revisit": True}),
    ("Ghost Notes", "Veronica Swift", "#3B4A4A", "not_for_me", 2, ["jazz"], "", 90, {"review": "Too polished for me."}),
    ("Random Access Memories", "Daft Punk", "#3B3B44", "not_for_me", 2.5, ["dance"], "", 260, {}),
]
TRACKS = [  # title, artist, album, tint, status, rating, tags, rec, added, extra
    ("Movement 6", "Floating Points", "Promises", "#2E3F5C", "explored", 5, ["late-night"], "", 14, {}),
    ("Love Is Everywhere", "Pharoah Sanders", "Thembi", "#5C4630", "planned", None, ["spiritual"], "", 0, {}),
    ("cold/mess", "Prateek Kuhad", "cold/mess (single)", "#4F5B6B", "explored", 5, ["hindi-indie"], "Ishaan", 210, {}),
    ("Hope", "Prateek Kuhad", "Hope (single)", "#4F5B6B", "explored", 4, ["hindi-indie"], "", 40, {}),
    ("Teardrop", "Massive Attack", "Mezzanine", "#2B4F7A", "explored", None, ["trip-hop"], "", 400, {}),
    ("Running Up That Hill", "Kate Bush", "Hounds of Love", "#6B3A4A", "explored", 4, ["art-pop"], "", 35, {}),
    ("Blue in Green", "Miles Davis", "Kind of Blue", "#22324A", "explored", 5, ["jazz"], "", 900, {}),
    ("Naima", "John Coltrane", "Giant Steps", "#3B3B44", "explored", 4, ["jazz"], "", 600, {}),
    ("Peace Piece", "Bill Evans", "Everybody Digs Bill Evans", "#2F3B4A", "explored", 5, ["jazz", "piano"], "", 300, {}),
    ("Lonnie's Lament", "John Coltrane", "Crescent", "#3B3B44", "explored", 2, ["jazz"], "", 250, {}),
    ("Smooth Operator", "Sade", "Diamond Life", "#5A4A3B", "explored", 1.5, ["not-jazz"], "", 500, {}),
    ("Glow", "Kelly Lee Owens", "Inner Song", "#4A4F59", "planned", None, ["electronic"], "", 9, {}),
    ("Turn Around", "Ruth Radelet", "Turn Around (single)", "#4A4F59", "planned", None, [], "", 1, {"phone": True}),
    ("Pyramids", "Frank Ocean", "channel ORANGE", "#5C3D30", "planned", None, ["r-and-b"], "Meera", 4, {}),
    ("Sun Is Shining", "Bob Marley", "Kaya", "#5A6B3B", "planned", None, [], "", 3, {}),
    ("Journey in Satchidananda", "Alice Coltrane", "Journey in Satchidananda", "#3D5A4A", "explored", 4.5, ["spiritual"], "", 120, {}),
]
# Track lists the user has entered (enables "unfinished albums", TSD §4.9)
TRACKLISTS = {
    "Promises": [f"Movement {i}" for i in range(1, 10)],
    "Hounds of Love": ["Running Up That Hill", "Hounds of Love", "The Big Sky", "Mother Stands for Comfort", "Cloudbusting",
                       "And Dream of Sheep", "Under Ice", "Waking the Witch", "Watching You Without Me", "Jig of Life",
                       "Hello Earth", "The Morning Fog"],
    "Bitches Brew": ["Pharaoh's Dance", "Bitches Brew", "Spanish Key", "John McLaughlin", "Miles Runs the Voodoo Down",
                     "Sanctuary", "Feio"],
    "Kind of Blue": ["So What", "Freddie Freeloader", "Blue in Green", "All Blues", "Flamenco Sketches"],
}
# Real track names for listening events only (not "added" track lists, so they don't count for unfinished albums)
PLAY_TRACKS = {
    "Journey in Satchidananda": ["Journey in Satchidananda", "Shiva-Loka", "Stopover Bombay", "Something About John Coltrane", "Isis and Osiris"],
    "Mezzanine": ["Angel", "Risingson", "Teardrop", "Inertia Creeps", "Black Milk"],
    "A Love Supreme": ["Acknowledgement", "Resolution", "Pursuance", "Psalm"],
    "Blue Train": ["Blue Train", "Moment's Notice", "Locomotion", "I'm Old Fashioned", "Lazy Bird"],
    "Carrie & Lowell": ["Death with Dignity", "Should Have Known Better", "Fourth of July", "Carrie & Lowell", "No Shade in the Shadow of the Cross"],
    "Moon Safari": ["La femme d'argent", "Sexy Boy", "All I Need", "Kelly Watch the Stars", "Talisman"],
    "Waltz for Debby": ["My Foolish Heart", "Waltz for Debby", "Detour Ahead", "Milestones"],
    "In Rainbows": ["15 Step", "Nude", "Weird Fishes/Arpeggi", "Reckoner", "House of Cards"],
    "Homogenic": ["Hunter", "Joga", "Bachelorette", "All Is Full of Love"],
    "Pet Sounds": ["Wouldn't It Be Nice", "God Only Knows", "Sloop John B", "Caroline, No"],
    "Remain in Light": ["Born Under Punches", "Once in a Lifetime", "The Great Curve", "Houses in Motion"],
    "Blackstar": ["Blackstar", "Lazarus", "Girl Loves Me", "I Can't Give Everything Away"],
    "Pink Moon": ["Pink Moon", "Place to Be", "Road", "Things Behind the Sun"],
    "Random Access Memories": ["Get Lucky", "Instant Crush", "Giorgio by Moroder"],
    "cold/mess (single)": ["cold/mess"],
}
EXTRA = {  # listened to, but not in the library
    ("Sun Ra", "Lanquidity"): ["Lanquidity", "Where Pathways Meet", "That's How I Feel"],
    ("Khruangbin", "Con Todo El Mundo"): ["Maria Tambien", "August 10", "Evan Finds the Third Room"],
    ("Nils Frahm", "Spaces"): ["Says", "Over There, It's Raining", "Hammers"],
    ("Arooj Aftab", "Vulture Prince"): ["Mohabbat", "Baghon Main", "Inayaat"],
    ("Anoushka Shankar", "Love Letters"): ["Lovable", "Unsaid", "Those Words"],
    ("Bonobo", "Migration"): ["Migration", "Kerala", "Break Apart"],
    ("Tame Impala", "Currents"): ["Let It Happen", "The Less I Know the Better", "Eventually"],
    ("Ritviz", "Ritviz (singles)"): ["Udd Gaya", "Liggi", "Baraat"],
}

items, artist_id = [], {}


def add_item(kind, name, **kw):
    it = {"id": len(items) + 1, "type": kind, "display_name": name, "spotify_uri": f"spotify:{kind}:{sid()}",
          "status": "planned", "priority": None, "rating": None, "review": "", "notes": "", "favourite": False,
          "revisit": False, "in_backlog": True, "recommended_by": "", "tags": [], "added_at": None,
          "explored_at": None, "reviewed_at": None, "provenance": {"display_name": "promoted"}}
    it.update(kw)
    items.append(it)
    return it


for name, tint, aff, status, note in ARTISTS:
    added = day(rnd.randint(30, 500), rnd.randint(9, 22))
    a = add_item("artist", name, tint=tint, status=status, notes=note, added_at=iso(added),
                 explored_at=iso(added + timedelta(days=rnd.randint(10, 25))) if status == "explored" else None,
                 in_backlog=status in ("planned", "in_progress"), affinity=aff)
    artist_id[name] = a["id"]


def mk(kind, rows):
    for title, artist, *rest in rows:
        if kind == "album":
            tint, status, rating, tags, rec, added, extra = rest
            album = None
        else:
            album, tint, status, rating, tags, rec, added, extra = rest
        added_at = day(added, rnd.randint(8, 23), rnd.randint(0, 59))
        if added_at > TODAY - timedelta(hours=1):
            added_at = TODAY - timedelta(hours=rnd.randint(2, 9))
        explored = status in ("explored", "not_for_me")
        cap = lambda d: min(d, TODAY - timedelta(hours=1))  # never in the future
        it = add_item(kind, title, artist=artist, album=album, artist_id=artist_id.get(artist), tint=tint,
                      status=status, rating=rating, tags=tags, recommended_by=rec, added_at=iso(added_at),
                      explored_at=iso(cap(added_at + timedelta(days=rnd.randint(2, 20)))) if explored else None,
                      in_backlog=status in ("planned", "in_progress"), review=extra.get("review", ""),
                      reviewed_at=iso(cap(added_at + timedelta(days=rnd.randint(3, 25)))) if extra.get("review") else None,
                      priority=extra.get("priority"), revisit=extra.get("revisit", False),
                      favourite=extra.get("favourite", False), via_phone=extra.get("phone", False))
        if kind == "album" and title in TRACKLISTS:
            it["tracklist"] = TRACKLISTS[title]


mk("album", ALBUMS)
mk("track", TRACKS)
by_name = {(i["type"], i["display_name"]): i for i in items}

# Phone captures still waiting for a name (TSD §4.2: display_name NULL)
captures = [
    {"id": 1, "raw_link": "open.spotify.com/album/" + sid(), "received_at": iso(day(0, 9, 41)), "source": "shortcut", "state": "name_needed"},
    {"id": 2, "raw_link": "spotify.link/" + sid()[:7], "received_at": iso(day(1, 22, 5)), "source": "shortcut", "state": "unresolved"},
]

# Status history (each explored item: planned -> in_progress -> final)
history = []
for it in items:
    added = datetime.fromisoformat(it["added_at"])
    seq = [("", "planned", added, "Shared from iPhone, named on PC" if it.get("via_phone") else "Added")]
    if it["status"] != "planned":
        seq.append(("planned", "in_progress", added + timedelta(days=1), ""))
    if it["status"] in ("explored", "not_for_me", "abandoned"):
        seq.append(("in_progress", it["status"], datetime.fromisoformat(it["explored_at"]) if it["explored_at"] else added + timedelta(days=5), ""))
    for f, t, at, note in seq:
        history.append({"item_id": it["id"], "from": f, "to": t, "at": iso(min(at, TODAY)), "note": note})
    if it["review"]:
        history.append({"item_id": it["id"], "from": "", "to": "review", "at": it["reviewed_at"], "note": "Review written"})

# ---------------------------------------------------------------- listening events
pool = []  # (track, artist, album, minutes, weight)
for it in items:
    if it["type"] == "track":
        pool.append((it["display_name"], it["artist"], it["album"], rnd.uniform(3, 7), 3))
PARTIAL = {"Promises": 6, "Hounds of Love": 7, "Bitches Brew": 2}  # in progress / abandoned: only these were played
for it in items:
    if it["type"] == "album" and it["status"] != "planned":
        names = it.get("tracklist") or PLAY_TRACKS.get(it["display_name"], [])
        names = names[:PARTIAL.get(it["display_name"], len(names))]
        for t in names:
            pool.append((t, it["artist"], it["display_name"], rnd.uniform(3, 9), 1))
for (ar, al), names in EXTRA.items():
    for t in names:
        pool.append((t, ar, al, rnd.uniform(3, 6), 1))
aff = {a[0]: a[2] for a in ARTISTS}
weights = [w * aff.get(p[1], 1) for (*p, w) in [(x[0], x[1], x[2], x[3], x[4]) for x in pool]]

NEW_ARTISTS = {"Pharoah Sanders": 6, "Kelly Lee Owens": 9, "Ruth Radelet": 2, "Arooj Aftab": 5}  # first heard N days ago
DORMANT = {"Bill Evans", "Nick Drake", "Sufjan Stevens"}
OPEN_SINCE = {(i["display_name"], i["artist"]): i["added_at"] for i in items
              if i["type"] != "artist" and i["status"] in ("planned", "in_progress")}
TRACK_URI = {(i["display_name"], i["artist"]): i["spotify_uri"] for i in items if i["type"] == "track"}


def uri_for(track, artist):
    return TRACK_URI.setdefault((track, artist), "spotify:track:" + sid())


HOUR_W = [2, 1, 0, 0, 0, 0, 1, 3, 6, 5, 3, 3, 4, 4, 4, 4, 5, 5, 6, 7, 9, 12, 13, 8]
events = []
start = TODAY - timedelta(days=365)
d = start
while d < TODAY:
    weekend = d.weekday() >= 5
    month_boost = 0.75 + 0.5 * ((d - start).days / 365)  # listening grows over the year
    n = int(rnd.gauss(26 if weekend else 22, 6) * month_boost)
    for _ in range(max(n, 0)):
        h = rnd.choices(range(24), weights=[w * (1.6 if weekend and 11 <= i <= 18 else 1) for i, w in enumerate(HOUR_W)])[0]
        t = d.replace(hour=h, minute=rnd.randint(0, 59))
        if t > TODAY:
            continue
        track, artist, album, mins, _ = rnd.choices(pool, weights=weights)[0]
        if artist in NEW_ARTISTS and t < TODAY - timedelta(days=NEW_ARTISTS[artist]):
            continue  # discovered recently
        if artist in DORMANT and t > TODAY - timedelta(days=150):
            continue  # loved, but not played lately
        started = OPEN_SINCE.get((album, artist)) or OPEN_SINCE.get((track, artist))
        if started and iso(t) < started:
            continue  # planned / in-progress records are only played after you added them
        skipped = rnd.random() < 0.17
        ms = rnd.randint(4000, 28000) if skipped and rnd.random() < 0.7 else int(mins * 60000 * (rnd.uniform(0.3, 0.9) if skipped else 1))
        source = "logger" if t > EXPORT_UNTIL and rnd.random() < 0.9 else ("export" if t <= EXPORT_UNTIL else None)
        if source is None:
            continue  # phone plays after the latest export are not known yet
        platform = "windows" if source == "logger" or rnd.random() < 0.55 else "ios"
        events.append({"ended_at": iso(t), "ms": ms, "track": track, "artist": artist, "album": album, "source": source,
                       "platform": platform, "skipped": skipped,
                       "private": source == "export" and rnd.random() < 0.02,  # only the export carries the flag (D24)
                       "counts": ms >= 30000, "spotify_uri": uri_for(track, artist)})
    d += timedelta(days=1)
events.sort(key=lambda e: e["ended_at"])
# Pin the "now playing" event and a few recent ones the designs reference
events.append({"ended_at": iso(TODAY), "ms": 252000, "track": "Love Is Everywhere", "artist": "Pharoah Sanders", "album": "Thembi",
               "source": "logger", "platform": "windows", "skipped": False, "private": False, "counts": True, "now_playing": True,
               "duration_ms": 571000, "spotify_uri": uri_for("Love Is Everywhere", "Pharoah Sanders")})

counted = [e for e in events if e["counts"] and not e["private"]]
lib_tracks = {(i["display_name"], i["artist"]): i for i in items if i["type"] == "track"}
lib_albums = {(i["display_name"], i["artist"]): i for i in items if i["type"] == "album"}


def item_for(e):
    return lib_tracks.get((e["track"], e["artist"])) or lib_albums.get((e["album"], e["artist"]))


def window(days):
    lim = iso(TODAY - timedelta(days=days)) if days else ""
    return [e for e in counted if e["ended_at"] >= lim]


def top(evts, key, n):
    c = Counter(key(e) for e in evts)
    ms = defaultdict(int)
    for e in evts:
        ms[key(e)] += e["ms"]
    return [{"name": k[0] if isinstance(k, tuple) else k, "by": k[1] if isinstance(k, tuple) else "", "plays": v,
             "hours": round(ms[k] / 3600000, 1)} for k, v in sorted(c.items(), key=lambda kv: -ms[kv[0]])[:n]]  # CAP-23: by time


def annotate(rows, kind):
    for r in rows:
        it = by_name.get((kind, r["name"]))
        r["item_id"] = it["id"] if it else None
        r["status"] = it["status"] if it else None
        r["rating"] = it["rating"] if it else None
    return rows


ranges = {"7d": 7, "30d": 30, "12m": 365, "all": 0}
charts = {k: {"artists": annotate(top(window(v), lambda e: e["artist"], 10), "artist"),
              "albums": annotate(top(window(v), lambda e: (e["album"], e["artist"]), 10), "album"),
              "tracks": annotate(top(window(v), lambda e: (e["track"], e["artist"]), 10), "track")} for k, v in ranges.items()}


def stats(days):
    cur, prev = window(days), [e for e in counted if iso(TODAY - timedelta(days=2 * days)) <= e["ended_at"] < iso(TODAY - timedelta(days=days))]
    first_seen = {}
    for e in counted:
        first_seen.setdefault(e["artist"], e["ended_at"])
    lim = iso(TODAY - timedelta(days=days))
    return {"plays": len(cur), "plays_prev": len(prev), "ms": sum(e["ms"] for e in cur), "ms_prev": sum(e["ms"] for e in prev),
            "new_artists": sum(1 for a, t in first_seen.items() if t >= lim),
            # one definition everywhere: a counted play whose record (track, else its album) has no rating from you
            "unrated_plays": sum(1 for e in cur if not (item_for(e) or {}).get("rating")),
            "rated_this_period": sum(1 for h in history if h["to"] == "review" and h["at"] >= lim) + 6}


months = defaultdict(int)
for e in window(365):
    months[e["ended_at"][:7]] += e["ms"]
monthly = [{"month": m, "hours": round(v / 3600000, 1)} for m, v in sorted(months.items())][-12:]
heat = [[0] * 24 for _ in range(7)]
for e in window(365):
    t = datetime.fromisoformat(e["ended_at"])
    heat[t.weekday()][t.hour] += 1
peak = max(max(r) for r in heat) or 1
heatmap = [[round(v / peak, 2) for v in r] for r in heat]
all_evts = [e for e in events if not e["private"]]
skip_rate = round(100 * sum(1 for e in all_evts if e["skipped"]) / len(all_evts), 1)
phone_share = round(100 * sum(1 for e in counted if e["platform"] == "ios") / len(counted), 1)

played_tracks = Counter((e["track"], e["album"]) for e in counted)
unfinished = []
for it in items:
    if it.get("tracklist") and it["status"] != "planned":
        played = sum(1 for t in it["tracklist"] if played_tracks.get((t, it["display_name"])))
        if played < len(it["tracklist"]):
            unfinished.append({"item_id": it["id"], "title": it["display_name"], "played": played, "total": len(it["tracklist"])})

plays_by_item = Counter()  # artist: all its plays; album: plays of its tracks; track: plays of that track
last_play = {}
for e in counted:
    for it in (by_name.get(("artist", e["artist"])), lib_albums.get((e["album"], e["artist"])), lib_tracks.get((e["track"], e["artist"]))):
        if it:
            plays_by_item[it["id"]] += 1
            last_play[it["id"]] = e["ended_at"]
takes = []
for it in items:
    p = plays_by_item.get(it["id"], 0)
    if it["rating"] and it["rating"] >= 4.5 and it["id"] in last_play and last_play[it["id"]] < iso(TODAY - timedelta(days=90)):
        takes.append({"item_id": it["id"], "kind": "loved_not_played", "line": f"You rated it {it['rating']:g}. Last played {datetime.fromisoformat(last_play[it['id']]).strftime('%d %b').lstrip('0')}."})
    if not it["rating"] and p >= 40 and it["type"] == "track":
        takes.append({"item_id": it["id"], "kind": "played_not_rated", "line": f"Played {p} times. Never rated."})
    if it["in_backlog"] and it["type"] == "album" and p == 0 and it["added_at"] < iso(TODAY - timedelta(days=20)):
        takes.append({"item_id": it["id"], "kind": "backlog_never_played", "line": f"In your backlog since {datetime.fromisoformat(it['added_at']).strftime('%d %b').lstrip('0')}. Never played."})

matched_recent = []
for e in [e for e in events if e["ended_at"] >= iso(TODAY - timedelta(days=3))][-60:]:
    it = item_for(e)
    matched_recent.append({**e, "item_id": it["id"] if it else None, "rating": it["rating"] if it else None})
matched_recent.reverse()
# desktop plays between the export's end and the oldest row in `recent`, shown as "N more" on Recent plays
earlier_desktop = sum(1 for e in events if e["source"] != "export" and iso(EXPORT_UNTIL) <= e["ended_at"] < matched_recent[-1]["ended_at"])
unmatched = [
    {"track": "Teardrop", "artist": "Massive Attack", "detail": "You have the track and the album Mezzanine", "choice": "Link to the track", "plays": 3},
    {"track": "Running Up That Hill", "artist": "Kate Bush", "detail": "You have the track and the album Hounds of Love", "choice": "Link to the track", "plays": 2},
    {"track": "Blue in Green", "artist": "Miles Davis", "detail": "Track and album both in your library", "choice": "Link to the track", "plays": 1},
]

year = [e for e in counted if e["ended_at"].startswith("2026")]
year_review = {"year": 2026, "plays": len(year), "hours": round(sum(e["ms"] for e in year) / 3600000), "top_artist": top(year, lambda e: e["artist"], 1)[0],
               "top_album": top(year, lambda e: (e["album"], e["artist"]), 1)[0], "explored": sum(1 for i in items if i["explored_at"] and i["explored_at"].startswith("2026")),
               "reviews": sum(1 for i in items if i["reviewed_at"] and i["reviewed_at"].startswith("2026")),
               "busiest_month": max(monthly, key=lambda m: m["hours"])["month"]}

# ---------------------------------------------------------------- playlists (export, R2 check-up) and Studio (R3)
def pl(name, tracks):
    return {"name": name, "tracks": [{"track": t, "artist": a} for t, a in tracks]}


playlists = [
    pl("Late night jazz", [("Journey in Satchidananda", "Alice Coltrane"), ("Blue in Green", "Miles Davis"), ("Naima", "John Coltrane"),
                           ("Blue in Green", "Miles Davis"), ("Lonnie's Lament", "John Coltrane"), ("Peace Piece", "Bill Evans"),
                           ("Movement 6", "Floating Points"), ("Smooth Operator", "Sade")]),
    pl("Sunday slow", [("cold/mess", "Prateek Kuhad"), ("Hope", "Prateek Kuhad"), ("cold/mess", "Prateek Kuhad"), ("Pink Moon", "Nick Drake"),
                       ("Teardrop", "Massive Attack"), ("Teardrop", "Massive Attack"), ("Peace Piece", "Bill Evans"), ("Hope", "Prateek Kuhad")]),
    pl("Run", [("Running Up That Hill", "Kate Bush"), ("Baraat", "Ritviz"), ("Let It Happen", "Tame Impala"), ("North American Scum", "LCD Soundsystem")]),
    pl("Hindi indie", [("cold/mess", "Prateek Kuhad"), ("Hope", "Prateek Kuhad"), ("Udd Gaya", "Ritviz")]),
    pl("Focus", [("Says", "Nils Frahm"), ("Kerala", "Bonobo"), ("Smooth Operator", "Sade"), ("Glow", "Kelly Lee Owens"),
                 ("Lonnie's Lament", "John Coltrane"), ("Hammers", "Nils Frahm"), ("Break Apart", "Bonobo")]),
    pl("Road trip 2025", [("Running Up That Hill", "Kate Bush"), ("The Less I Know the Better", "Tame Impala"), ("Sun Is Shining", "Bob Marley")]),
]
rating_of = {i["display_name"]: i["rating"] for i in items if i["type"] == "track"}
tags_of = {i["display_name"]: i["tags"] for i in items if i["type"] == "track"}
never_played = {(t["track"], t["artist"]) for p in playlists for t in p["tracks"]} - {(e["track"], e["artist"]) for e in counted}
for p in playlists:
    seen = set()
    for i, t in enumerate(p["tracks"]):
        key = (t["track"], t["artist"])
        t["position"] = i + 1
        t["rating"] = rating_of.get(t["track"])
        t["tags"] = tags_of.get(t["track"], [])
        t["duplicate_of"] = next((j + 1 for j, u in enumerate(p["tracks"][:i]) if (u["track"], u["artist"]) == key), None)
        t["never_played"] = key in never_played
        seen.add(key)
    p["dupes"] = sum(1 for t in p["tracks"] if t["duplicate_of"])
    p["low"] = sum(1 for t in p["tracks"] if t["rating"] is not None and t["rating"] <= 2)

# ---------------------------------------------------------------- source A: Spotify-provided (display only, never in insights)
spotify = {
    "connected": True, "renew_by": "2027-04-14",
    "top_tracks": {r: [{"rank": n + 1, "track": t["name"], "artist": t["by"],
                        "in_library": (t["name"], t["by"]) in lib_tracks} for n, t in enumerate(top(window(d), lambda e: (e["track"], e["artist"]), 20))]
                   for r, d in {"4w": 28, "6m": 182, "1y": 365}.items()},
    "top_artists": {r: [{"rank": n + 1, "artist": a["name"], "in_library": a["name"] in artist_id}
                        for n, a in enumerate(top(window(d), lambda e: e["artist"], 20))] for r, d in {"4w": 28, "6m": 182, "1y": 365}.items()},
    # GET /me/player/recently-played: finished plays only, no device field, no "now playing"
    "recent": [{"track": e["track"], "artist": e["artist"], "at": e["ended_at"], "spotify_uri": e["spotify_uri"],
                "in_library": (item_for(e) or {}).get("type")} for e in reversed([e for e in counted if not e.get("now_playing")][-50:])],
}

backlog = [i for i in items if i["in_backlog"]]
data = {
    "meta": {"generated_for": "Runout design prototype and M2 mocks", "seed": SEED, "today": iso(TODAY),
             "export_until": iso(EXPORT_UNTIL), "note": "Synthetic. Spotify IDs are random; no real export values."},
    "items": items, "captures": captures, "history": history,
    "saved_filters": [{"id": 1, "name": "5 star jazz", "definition": {"tags": ["jazz"], "min_rating": 5}},
                      {"id": 2, "name": "Late night, explored", "definition": {"tags": ["late-night"], "status": "explored"}}],
    "counts": {"library": len([i for i in items]), "albums": sum(1 for i in items if i["type"] == "album"),
               "tracks": sum(1 for i in items if i["type"] == "track"), "artists": sum(1 for i in items if i["type"] == "artist"),
               "backlog": len(backlog), "in_progress": sum(1 for i in items if i["status"] == "in_progress"),
               "revisit": sum(1 for i in items if i["revisit"]), "playlists": len(playlists)},
    "listening": {"recent": matched_recent, "earlier_desktop": earlier_desktop, "unmatched": unmatched, "charts": charts,
                  "item_stats": {str(k): {"plays": v, "last": last_play.get(k)} for k, v in plays_by_item.items()},
                  "export_sample": [{**e, "item_id": (item_for(e) or {}).get("id")} for e in events if e["source"] == "export" and e["platform"] == "ios"][-6:][::-1],
                  "stats": {k: stats(v or 3650) for k, v in ranges.items()}, "monthly": monthly, "heatmap": heatmap,
                  "skip_rate": skip_rate, "phone_share": phone_share, "artists_total": len({e["artist"] for e in counted}),
                  "plays_total": len(counted), "unfinished": unfinished, "takes": takes, "year_review": year_review,
                  "private_excluded": sum(1 for e in events if e["private"]),
                  "freshness": {"logger_last": iso(TODAY), "export_until": iso(EXPORT_UNTIL)},
                  "import_preview": {"music": sum(1 for e in events if e["source"] == "export"), "podcasts_skipped": 1308,
                                     "overlap": 312, "playlists": len(playlists), "private": sum(1 for e in events if e["private"] and e["source"] == "export"),
                                     "from": events[0]["ended_at"][:10], "until": iso(EXPORT_UNTIL)[:10]}},
    "playlists": playlists, "spotify": spotify,
    "settings": {"library_path": "%APPDATA%\\app.runout.desktop\\library.db", "backup_dir": "Documents\\Runout\\Backups",
                 "backups": [{"at": iso(day(0, 9)), "mb": 14.2}, {"at": iso(day(7, 9)), "mb": 13.9}, {"at": iso(day(14, 9)), "mb": 13.6}],
                 "logger": {"on": True, "autostart": True, "include_private": False, "paused_until": None},
                 "capture": {"on": True, "address": "192.168.1.24:47821", "code": "482 913"},
                 # Runout Inbox (TSD §5.3, D33): what "Check now" finds waiting in the phone's Inbox playlist
                 "inbox": {"state": "ready", "last_check": iso(day(0, 9, 30)), "imported_last": 0,
                           "waiting": [{"type": "album", "added_at": iso(day(0, 8, 12))}]}},
    "usage": {"capture_median_s": 6, "days_opened_this_week": 5, "searches_this_week": 11, "rediscover_visits_this_week": 4},
}
# ---------------------------------------------------------------- consistency guarantees (also checked by scripts/check.py)
for it in items:
    if it["type"] == "track" and lib_albums.get((it["album"], it["artist"])):
        al = lib_albums[(it["album"], it["artist"])]
        assert plays_by_item[al["id"]] >= plays_by_item[it["id"]], f"album {al['display_name']} has fewer plays than its track"
    if it["type"] == "album" and by_name.get(("artist", it["artist"])):
        assert plays_by_item[artist_id[it["artist"]]] >= plays_by_item[it["id"]], f"artist has fewer plays than {it['display_name']}"
    assert any(h["item_id"] == it["id"] for h in history), f"{it['display_name']} has no history"
for kind in ("artists", "albums", "tracks"):
    for a, b in (("7d", "30d"), ("30d", "12m"), ("12m", "all")):
        big = {(r["name"], r["by"]): r["plays"] for r in top(window(ranges[b]), (lambda e: e["artist"]) if kind == "artists" else (lambda e: (e["album"], e["artist"])) if kind == "albums" else (lambda e: (e["track"], e["artist"])), 10_000)}
        for r in charts[a][kind]:
            assert r["plays"] <= big[(r["name"], r["by"])], f"{kind} {r['name']}: {a} > {b}"
for r in charts["all"]["albums"]:
    it = lib_albums.get((r["name"], r["by"]))
    assert not it or plays_by_item[it["id"]] == r["plays"], f"album {r['name']}: chart and item_stats disagree"
assert all(e["source"] == "export" for e in events if e["private"]), "private plays outside the export"
for rng in charts.values():
    for kind, rows in rng.items():
        assert all(x["hours"] >= y["hours"] for x, y in zip(rows, rows[1:])), f"{kind} chart not in order of the hours shown"
assert earlier_desktop > 0, "Recent plays would show a gap between the export and the logger"
OUT.mkdir(parents=True, exist_ok=True)
text = json.dumps(data, ensure_ascii=False, indent=1)
(OUT / "mock-data.json").write_text(text, encoding="utf-8")
(OUT / "mock-data.js").write_text("// Generated by scripts/gen_mock_data.py. Do not edit.\nwindow.RUNOUT_MOCK = " + text + ";\n", encoding="utf-8")
print(f"items={len(items)} backlog={len(backlog)} events={len(events)} counted={len(counted)} "
      f"7d_plays={data['listening']['stats']['7d']['plays']} size={len(text) // 1024}KB")
