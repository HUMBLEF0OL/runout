# Your Actions — Hard Stops Only

These are the only points where I need you. Everything else I do and record myself.

A **hard stop** is something I cannot do, for one of these reasons:
- it needs **your identity**: your Spotify login, your email or a legal acceptance in your name
- it needs **your physical device**: your iPhone
- it needs a **Windows security prompt** that only you can click
- it needs **your judgement** of the product after real use

When I reach one, I stop and point you to the item here. Each item says what to do, how long it takes, and what to send back.

| # | Action | When | Takes | Blocks |
|---|---|---|---|---|
| U-1 | Request your two Spotify data exports | **Now (M0)** | 2 min | M4 import · **done** (Extended Streaming History arrived 19 Oct 2026; Account data not seen yet) |
| U-2 | Download the exports and tell me where they are | When Spotify's emails arrive | 5 min | M4 import · **done 19 Oct 2026** (`Downloads\my_spotify_data.zip`) |
| U-9 | Approve the screen designs | End of M1 | ~30 min | UI build (M2) · **done 18 Oct 2026** |
| U-3 | Create the iPhone Shortcut | M6 | 5 min | Phone capture |
| U-4 | Allow the app through Windows Firewall | M6 | 10 s | Phone capture |
| U-5 | Use the app daily for 2 weeks, then say go/no-go | End of M6 | 2 weeks | M7 integration |
| U-6 | Create the Spotify developer app | Start of M5 | 5 min | Release 3 · **done 19 Oct 2026** (redirect `http://127.0.0.1:8888/callback`; client ID kept out of the repo) |
| U-7 | Approve the Spotify login once | M5 | 1 min | Release 3 · **done 19 Oct 2026** (gate login); live Studio check on a test playlist still open |
| U-8 | *(Optional)* Ask Spotify developer support one question | Before M5 | 5 min | Nothing |

---

## U-1 — Request your Spotify data exports (do this now)

**Why only you:** it needs your Spotify login, and Spotify confirms the request by email.

Steps:
1. Open **https://www.spotify.com/account/privacy/** and log in.
2. Scroll to **Download your data**.
3. Tick **Account data** and **Extended streaming history**. They may be separate requests; do both.
4. Click **Request data**.
5. Open the confirmation email from Spotify and click the link.

**Send back:** "U-1 done" and the date you requested.

**Note:** Spotify doesn't publish how long the exports take. Your request is how we find out (BRD O2). Measured so far: the Extended streaming history took about 1 day; the Account data export and allowed request frequency are still open.

## U-2 — Download the exports

**Why only you:** the download link goes to your email and needs your login.

Steps:
1. When each "your data is ready" email arrives, download the zip.
2. Save it **outside the project folder**, e.g. `C:\Users\<you>\Documents\spotify-export\`. This keeps it out of git.

**Send back:** the folder path.

**What I do with it:**
- I read it locally to check the file format (TSD T1) and to import it.
- No real values from it are ever committed to the repository.

## U-3 — Create the iPhone Shortcut (M6)

**Why only you:** it has to be built on your iPhone.

I'll give you the exact values on screen when M6 reaches this step: the PC's address, port and pairing code. The Shortcut has three actions:
1. **Receive** URLs from the **Share Sheet**.
2. **Get Contents of URL**:
   - URL `http://<PC-IP>:47821/capture`
   - Method **POST**
   - Request Body **JSON**: `code` = your pairing code, `link` = Shortcut Input.
3. **Show Result**.

Name it "Save to Runout" and turn on **Show in Share Sheet**.

**Test:** in Spotify on your iPhone, Share → Save to Runout, while on the same Wi-Fi as the PC.
- The first time, iOS asks whether Shortcuts may **find and connect to devices on your local network**. Tap **Allow**. If you denied it earlier, turn it on under Settings → Privacy & Security → Local Network → Shortcuts.
- The song appears in the backlog marked “name needed”. You confirm the name next time you're at the PC.

**Send back:** "U-3 done", or the error the Shortcut shows.

## U-4 — Allow through Windows Firewall (M6)

**Why only you:** Windows shows this security prompt only to you.

When you first enable phone capture, Windows asks whether to allow the app on networks:
- Tick **Private networks** only.
- Click **Allow**.

Also make sure your home Wi-Fi is set to **Private**: Settings → Network & internet → Wi-Fi → your network → Network profile type → **Private network**. On a Public profile, Windows blocks the phone even after you click Allow.

**Send back:** nothing, unless you clicked Cancel.

## U-5 — Two weeks of daily use, then go/no-go (end of M6)

**Why only you:** whether the app is useful is your call.

Use the app normally for 2 weeks. Then I'll show you the success metrics from your own data (BRD §15). You answer:
- **Go** to M7 (wiring the listening and Spotify screens),
- **Fix first**: list what annoyed you, or
- **Stop**.

## U-9 — Approve the screen designs (end of M1)

**Why only you:** whether the screens feel right is your call, and M2 builds exactly what you approve.

I'll walk you through every screen in the design prototype (`docs/design/`), journey by journey. You answer **Approve**, or list what to change. Changes after approval go through a spec change.

**Send back:** "U-9 approved", or your changes.

**Done:** approved 18 Oct 2026, together with the M2 side-by-side review of the built screens.

## U-6 — Create the Spotify developer app (start of M5)

**Why only you:**
- It needs your Spotify login.
- It needs your acceptance of Spotify's Developer Terms, which is a legal agreement in your name.
- It needs your Premium subscription.

Steps:
1. Open **https://developer.spotify.com/dashboard** and log in with the Premium Student account.
2. Accept the Developer Terms if asked.
3. **Create app**:
   - Name: `Runout`
   - Description: `Personal local music organizer`
   - Redirect URI: `http://127.0.0.1:8888/callback`. The dashboard rejects it without a port; the app still uses a free port at login, which Spotify allows for `127.0.0.1` (T3, verified 19 Oct 2026).
   - APIs used: tick **Web API**
   - Save.
4. Open the app → **Settings** → copy the **Client ID**.
5. If later asked, add your own Spotify email under **User Management**.

**Send back:** the Client ID. It isn't a secret: the app uses PKCE login, so there is no client secret to store or share.

**Then I'll:**
- Run one API call. Its result answers BRD O1, whether your Student plan qualifies.
- If it fails because of the plan, Release 3 pauses and the decision comes back to you.

**Note:** I'll re-check the redirect-URI rules (TSD T3) just before this step and update the value here if Spotify has changed them.

## U-7 — Approve the Spotify login (M5)

**Why only you:** it's your account consent.

The app opens a Spotify page in your browser listing the permissions. Check that it says *Runout*, then click **Agree**.

This repeats **every 6 months**, which is Spotify's rule (BRD S7).

**Send back:** nothing. The app confirms the connection.

**Still to do for M5 (live Studio check, ~10 min):**
1. In Spotify, make a **test playlist** you don't care about: ~15 tracks, with one track added twice.
2. Set `RUNOUT_SPOTIFY_CLIENT_ID` to your Client ID in your environment (it stays out of the repo).
3. Run `cargo run --example studio_roundtrip -- spotify:playlist:<id>` in `src-tauri`, approve the login in the browser, check the cut in Spotify, press Enter, check it's restored.
4. Run it again with `--conflict` at the end: when it pauses, remove or move any track in Spotify, press Enter; it should print `PASS: apply blocked with conflict`.

**Send back:** the playlist link, or just "done" with what you saw.

## U-8 — *(Optional)* Ask Spotify developer support (before M5)

**Why only you:** it has to come from your developer account.

This is optional. The BRD already accepts the risk (§11.1, D26).

If you want certainty, post on the Spotify for Developers community or contact support:

> "In a personal, single-user Development Mode app, may I display statistics computed from my own Extended Streaming History export (requested via Account → Privacy), kept separate from any Web API data?"

**Send back:** the answer, if you get one. I'll update BRD §11.1.

---

## Not hard stops (I handle these)

- Tech decisions, design, code, tests and docs.
- Installing project packages (npm and Rust crates). You may see a tool-permission prompt.
- Building and installing the app locally. Locally built files carry no "downloaded from internet" mark, so Windows SmartScreen shouldn't appear.
- Running Playlist Studio checks, only ever on the **test playlist** you make (U-7); I never edit a real playlist.
- Reading the Windows "now playing" data while you listen.

## When I will stop and come back to you

I'll also stop, even outside the list above, when:
- **Spending money:** anything that would cost money.
- **Scope or compliance changes:** Spotify changes its rules or API in a way that changes what the BRD promises.
- **The export doesn't match what the BRD assumed:** its real format is missing something the BRD relies on.
- **Your Student plan doesn't qualify:** the U-6 test shows it isn't accepted for developer apps.
