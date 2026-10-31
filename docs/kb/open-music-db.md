# Open music database (MusicBrainz, Wikidata) — T6 / O8

Facts for M10 WI 9 Genres & decades (design [M10-insights-v3.md](../design/M10-insights-v3.md) §7, TSD §4.14). U-11 approved 26 Oct 2026: artist and album **names** only. Format: fact — **status**, date, source.

## Licence
- **MusicBrainz core data is CC0** (public domain): artists (name, sort name, **areas**, type, begin/end dates, aliases, MBID), releases (title, artist credit, **date**, country, ...), release groups (title, artist credit, type, MBID), areas, and the genre *list* itself (genre names). — **Documented**, 2026-10-26, https://wiki.musicbrainz.org/MusicBrainz_Database and https://wiki.musicbrainz.org/About/Data_License
- **MusicBrainz tags, including genre associations, are supplementary data: CC BY-NC-SA 3.0** ("user submitted annotations, tags (including genre associations) and ratings; derived statistics; search indexes"). Non-commercial use only, MusicBrainz must be credited, derivative works under the same licence. So "artist X is tagged rock" is BY-NC-SA, the name "rock" is CC0. The design's belief is confirmed. — **Documented**, 2026-10-26, same pages.
- **Fits Runout:** BRD §11.1/D26 make Runout private, single-user, non-commercial (BRD line "The application is for one person's private, non-commercial use"), and the enrich tables are never redistributed. Obligation that remains: **attribution** wherever genres show (e.g. "Genres from MusicBrainz, CC BY-NC-SA 3.0"). The design mock's "From: ... with genres from MusicBrainz (community tags)" line covers it if it names the licence. No scope or compliance change. — **Interpretation**, 2026-10-26.
- Release-group `first-release-date` is computed from its releases' dates (core). Whether MusicBrainz counts the computed value as a "derived statistic" is not stated; either way the non-commercial use above is allowed. — **Interpretation**, 2026-10-26.
- **Wikidata structured data (items, properties) is CC0**; no attribution required. — **Documented**, 2026-10-26, https://www.wikidata.org/wiki/Wikidata:Licensing

## MusicBrainz rules
- **Rate limit:** at most **1 request per second per IP** on average (a client over the rate gets **all** requests refused with **503** until it drops back, not just the excess); global cap 300 req/s; "never make more than ONE call per second" or the IP may be blocked. Do not schedule bulk lookups at a fixed clock time; spread background work. Do not poll for changes. — **Documented**, 2026-10-26, https://wiki.musicbrainz.org/MusicBrainz_API/Rate_Limiting and https://wiki.musicbrainz.org/MusicBrainz_API
- **User-Agent:** required, "with enough information ... to contact the application maintainers", format `Application name/<version> ( contact-url )` or `( contact-email )`. Blank, `Java`, `Python-urllib`, `Jakarta Commons-HttpClient` count as "anonymous" and share a throttled pool. — **Documented**, 2026-10-26, Rate_Limiting page.
- **No auth / no key** for reads; JSON via `fmt=json` (or `Accept: application/json`). — **Documented**, 2026-10-26, MusicBrainz_API page.
- **Latency:** search requests took ~1–5 s each on 26 Oct 2026 (probe below), so the real pace is under 1 req/s anyway; the time for a first fill of 150 artists (search + lookup each) is not measured yet (T6). — **Verified** (per-request latency) / **Open** (first-fill time), 2026-10-26.

## Endpoints (all `https://musicbrainz.org/ws/2/`, host `musicbrainz.org` only)
- **Artist search (name only):** `GET /ws/2/artist?query=<lucene>&limit=5&fmt=json` (limit 1–100, default 25). Results carry `score` (0–100), `name`, `sort-name`, `aliases`, `country`, `area`, `life-span`, and community `tags` (no `genres` in search results). Quote the name (`"..."`) and escape Lucene specials, or pass `dismax=true`. — **Verified**, 2026-10-26 (probe), https://wiki.musicbrainz.org/MusicBrainz_API/Search
- **Genres for a matched artist / release group:** lookup `GET /ws/2/artist/<MBID>?inc=genres+tags&fmt=json` (also `release-group/<MBID>?inc=genres`). `genres` = tags on MusicBrainz's genre list, each with a vote `count`; `inc=tags` returns all tags. One extra request per entity. — **Verified**, 2026-10-26.
- **Artist country / area:** `country` (ISO 3166-1 code, may be absent) and `area` (`name`, `type`) on both search results and lookups; no extra `/area` call needed for a country label. — **Verified**, 2026-10-26.
- **Album (release group) + first release year:** `GET /ws/2/release-group?query=releasegroup:"<album>" AND artist:"<artist>"&limit=5&fmt=json`; each result has `first-release-date` (`YYYY`, `YYYY-MM` or `YYYY-MM-DD`), `primary-type`, `score`. One request per album. — **Verified**, 2026-10-26.
- Genre list: `GET /ws/2/genre/all?fmt=txt` (all genre names, CC0) if Runout wants to map tags itself. — **Documented**, 2026-10-26.

## Wikidata fallback
- **Endpoint:** `https://www.wikidata.org/w/api.php` (host `www.wikidata.org`): `action=wbsearchentities&search=<name>&language=en&type=item&limit=5&format=json` finds candidates (label + description such as "British band"); `action=wbgetentities&ids=<Q..>&props=claims|labels&format=json` gives claims: genre **P136**, country of origin **P495**, citizenship **P27**, formation location **P740**, inception **P571**, publication date **P577** (albums), and **P434 MusicBrainz artist ID** (cross-check). Claim values are QIDs, so labels need one more `wbgetentities` (up to 50 ids per call). No SPARQL host needed. — **Verified**, 2026-10-26 (public sample, not user data), https://www.mediawiki.org/wiki/API:Etiquette
- **Rules:** descriptive User-Agent with contact info (`<client>/<version> (<contact>)`), else **403**: a request with an empty User-Agent got **HTTP 403** on 26 Oct 2026. Requests in series, not parallel; no hard read limit; `maxlag` for non-interactive work; gzip. — **Verified** (403) / **Documented** (rest), 2026-10-26, https://foundation.wikimedia.org/wiki/Policy:Wikimedia_Foundation_User-Agent_Policy and API:Etiquette.

## Match rate on the user's real library (probe, 26 Oct 2026)
Copy of the live `library.db` (temp dir, read-only, outside the repo; deleted after); events with `reconciled_with IS NULL`; top 150 artists by listening time; names only, HTTPS, ≤ 1 req/s, own User-Agent. "Confident" = an exact name or alias match (case, accents and punctuation folded) that is the top result with score ≥ 90 and no second artist of the same name; "same name" = the exact name is the top hit but other artists share it (name-only can't pick). Aggregates only; no names or values recorded.

**Open (T6):** results not recorded yet. The probe must be re-run (with the user's approval to use their artist names) and only the aggregate rates written here.

## Gotchas
- A name-only search can't tell apart artists who share a name: keep them as a separate "same name" class (pick the top-scored, or show as unknown) rather than guessing silently.
- The User-Agent needs a contact (URL or email) by both services' rules. Runout has no public site; the contact must not be anything about the user (no email from the user's account). Decide the string before building WI 9 (proposal: `Runout/<version> ( <project URL> )`).
