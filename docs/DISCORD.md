# Discord Rich Presence

Aurora Viewer can show what you are doing on your Discord profile: the
"Playing Aurora Viewer" card that your friends and the members of your
servers see under your name, with the region you are in and, if you choose,
your avatar name.

It works like the Legacy Viewer's Discord option and is **off by default**.
Nothing is sent while it is off. When it is on, the viewer only talks to the
Discord app running on your own computer; Discord itself publishes the card.

## Turn it on

1. Start the Discord desktop app on the same computer and sign in. The web
   version of Discord (in a browser) cannot receive the activity.
2. In Discord, open **User Settings › Activity Privacy** and turn on
   **Share your detected activities with others**.
3. In Aurora, open **Preferences › Privacy › Discord** and turn on
   **Show my activity on Discord**.
4. Log in. Within a few seconds the **Connection** line says
   "Connected to Discord as …" and your profile shows the card.

Turning the switch off, logging out or quitting the viewer clears the card
at once.

## What the card shows

| Line | What it says | Setting |
|---|---|---|
| Title | Playing **Aurora Viewer** (the name of the Discord application) | — |
| Details | Your display name and user name | **Share my name** (off) |
| State | `Region (x, y, z)`, or **Hidden Region** | **Share the region of** (General regions) and **Never share these regions** |
| Party | The number of avatars in the region, "(3 of 40)", when the region's limit is known | shown with the region only |
| Elapsed | Time since you logged in | — |
| Large picture | Second Life, or OpenSimulator on another grid, with the grid's name | — |
| Small picture | The Aurora logo, "via Aurora Viewer" | — |
| Button | **Aurora Viewer**, a link to the project's website, for the people looking at your profile | — |

The region and your position show only when **all** of these are true:

- the region's rating is one you chose under **Share the region of**
  (General only by default; "No region" never shows one);
- the region is not in **Never share these regions** (names compared
  ignoring case);
- RLVa does not hide your location (`@showloc`).

Your name shows only when **Share my name** is on and RLVa does not hide
names (`@shownames`, `@shownametags`).

## Troubleshooting

| The Connection line says | What to do |
|---|---|
| Shown once you are in world | Log in: the card is only shown in world. |
| Discord is not running on this computer | Start the Discord desktop app. The viewer tries again every 15 seconds. On Linux, a Flatpak or Snap Discord must be allowed to create its socket (the official Flatpak and Snap packages do); a Discord in a browser cannot be reached. |
| No Discord application | This build has no application id built in: create one (below) and type its id in **Discord application id**. |
| Discord refused: … | The application id is wrong or the application was deleted: check the id on the Developer Portal. |
| Connected, but no card on the profile | Check **Share your detected activities with others** in Discord's Activity Privacy settings, and that you are not invisible in Discord. Your own profile shows the card; the button only appears to other people. |

Two viewers running at once share one card: Discord shows the last one that
wrote.

## Your own Discord application

The card's title ("Playing …") and its pictures come from a Discord
application. The published viewer has one built in. A build of your own
(a fork, a development build) has none until you create one; this takes a
few minutes and is free.

1. Open the [Discord Developer Portal](https://discord.com/developers/applications)
   and sign in with your Discord account.
2. **New Application**: the name you type is the card's title, for example
   `Aurora Viewer`. Accept the terms and create it.
3. On the **General Information** page, copy the **Application ID** (a
   number of 17 to 20 digits). It is not a secret: every program with a
   Discord card ships its own.
4. Open **Rich Presence › Art Assets** and upload three images of at least
   512 × 512 pixels, under exactly these names:

   | Name | Image |
   |---|---|
   | `aurora` | the Aurora logo (`assets/branding/aurora-logo.png` in the repository) |
   | `secondlife` | a Second Life picture of your choice |
   | `opensimulator` | an OpenSimulator picture of your choice |

   Discord can take several minutes to show newly uploaded images.
5. Give the id to the viewer, in one of two ways:
   - **For you alone**: paste it in Preferences › Privacy › Discord ›
     **Discord application id**. It is kept in your settings and wins over
     the built-in one; empty the field to go back to the built-in one.
   - **Built into a build**: set the environment variable
     `AURORA_DISCORD_APP_ID` while building
     (`AURORA_DISCORD_APP_ID=123… cargo build -p aurora-viewer`). The
     Release workflow passes the repository variable of the same name
     (GitHub › Settings › Secrets and variables › Actions › **Variables**
     tab › `AURORA_DISCORD_APP_ID`), so the published zip carries it.

## Testing without the grid

In demo mode, `AURORA_DEMO_DISCORD=1` turns the option on for the run
without saving it: with Discord running, the profile shows the demo's plaza.

```powershell
$env:AURORA_DEMO = "1"
$env:AURORA_DEMO_DISCORD = "1"
cargo run -p aurora-viewer -- --title "Test discord"
```

The log says `discord: connected`, `discord: disconnected` and why an
activity was refused; it never writes your Discord name.
