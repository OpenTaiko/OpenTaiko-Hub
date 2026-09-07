# Changelog

You can update either through the OpenTaiko Hub, or directly from the releases tab:
- https://github.com/OpenTaiko/OpenTaiko-Hub/releases

## [0.2.4] - 2026-09-07 (Alpha)

- [Chore] Upgraded to Svelte 5, Skeleton UI 5 with Tailwind 4, and Tauri 2.11
- [Fix] Documentation tab showed "content.js failed to load" with the latest experimental builds

## [0.2.3] - 2026-09-06 (Alpha)

- [Fix] Songs can be downloaded concurrently again: since 0.2.0 every Download / Update button was locked while any single song was downloading
- [Fix] A song download that fails or throws now releases its row (the button no longer stays stuck as a progress bar) and cleans up its temporary folder

## [0.2.2] - 2026-08-07 (Alpha)

- [Chore] Max list points now decay by 0.95 up to rank 20, 0.96 from rank 21 to 50 and 0.98 from rank 51, so later ranks lose less points (matches the website)
- [Fix] The "Update OpenTaiko Hub" button did nothing when clicked
- [Fix] Songs were not recognized when a .tja file was placed directly inside the Songs folder

## [0.2.1] - 2026-08-03 (Alpha)

- [Fix] Fix local instance missing in the instances list when updating the Hub to 0.2 (can still be added with Attach Existing Instance but it is not an intuitive method)

## [0.2.0] - 2026-08-02 (Alpha)

- [Feat] Multiple OpenTaiko instances: create new instances (OpenTaiko-[uuid] folders), attach existing installs, and switch between them from the new instance bar
- [Feat] Stable and Experimental build channels; when the latest release is older than 0.6.1, the "InDev 0.6.1" experimental build is offered (fetches the branch source at its head commit and builds it locally with the .NET SDK 8, on Windows and Linux; updates track new branch commits)
- [Feat] Docs tab: builds from 0.6.1 show their bundled documentation (docs/index.html) rendered fully locally inside the Hub, per instance
- [Feat] Shared Songs library between all instances (Config.ini TJAPath is linked automatically, written as Shift_JIS); songs found inside instances can be transferred to the shared library in one click
- [Feat] Song library tree view per genre folder, including custom charts/songs
- [Feat] Saves tab per instance to import/export save files between OpenTaiko instances
- [Enhancement] Song scanning rewritten natively: async, near-instant, with live progress and per-song status streaming in
- [Enhancement] Bulk download now detects and updates outdated box.def and default.png files by comparing against the soundtrack repository
- [Enhancement] Skins/characters/puchicharas are scoped to the selected instance; downloads are disabled on experimental builds (they ship with their own assets)
- [i18n] Added French, Spanish, German, Dutch, Korean and Russian translations (translations other than French are to review/improve)

## [0.1.20] - 2026-04-13 (Alpha)

- [Chore/i18n] Correct all instances of チャート to 譜面 in the 日本語 (Japanese) translation
- [Fix] Inconsistent tauri version fixed by pinning minor version
- [Enhance] Tool tab layout (fix header size hierarchy; revise titles for readability; prevent image distortion)
- [Enhance] Retake tool screenshots and refine description
- [i18n] adding missing translations; adjust zh-TW translation
- [Enhance] Fix double parentheses for [Fix] legend
- [Enhance] \[Enhancement] -> [Enhance(ment)]
- [Chore] Deduplicate duplicated codes
- [Fix] Debug dark mode switch broken
- [Enhance] Replace HoF panel "close" text button with an x-mark button

## [0.1.19] - 2026-04-12 (Alpha)

- [Chore] Sort HoF plays by list points and translate clear statuses

## [0.1.18] - 2026-04-12 (Alpha)

- [Enhancement] Add artists information directly on the song list visible by expanding the song bar

## [0.1.17] - 2026-04-11 (Alpha)

- [Enhancement] Better HoF display and directly use the hof.db3 database to get the HoF ranks
- [Fix] Play icon sometimes disappearing when changing theme

## [0.1.16] - 2026-04-09 (Alpha)

- [Enhancement] Faster skin downloads

## [0.1.15] - 2026-04-09 (Alpha)

- [i18n] Multilanguage support (comes with English, Japanese, and Chinese (Simplified and Traditional))
- [Enhancement] Keep theme settings persistant between OpenTaiko Hub instances and after updates/reinstallation

## [0.1.14] - 2026-04-08 (Alpha)

- [Enhancement] Faster local songs status processing for users with lots of custom charts in their song folder.

## [0.1.12] - 2026-04-01 (Alpha)

- [Fix] Fix broken download for songs having subfolders with TJAP3ext image gimmicks.

## [0.1.7] - 2025-09-20 (Alpha)

- [Feat/Theme] Added the "OpTk Hub Themes" tab with the following themes:
    - OpenTaiko Hub themes: "Gleaming Sky", "888", "Deceiver", "Onyx", "Pearl", and "OpenTaiko-Kun".
    - Skeleton preset themes: "Legacy" (default), "Wintry", "Modern", "Rocket", "Seafoam", "Vintage", "Sahara", "Hamlindigo", "Gold Nouveau", and "Crimson".
- [Feat] Added a guide for the changelog terms.
- [Feat] Made the OpenTaiko Hub window resizable.
- [Enhancement] Added button icons.
- [Enhancement] A bunch of UI changes.
- [Enhancement] Added the OpenTaiko Blog to the "Links" tab.
- [Enhancement] Added a banner, header, and icon to the OpenTaiko Hub installer.
- [Chore] Changed the updater code to pull the app version from "tauri.conf.json".
- [Chore] Renamed the "OpenTaiko Version" tab to "Home".


## [0.1.6] - 2025-07-27 (Alpha)

- [Feat] Added the current version number to the OpenTaiko Hub.
- [Feat] Added a update system for the OpenTaiko Hub.
- [Feat] Added the "Secrets" tab. (Maybe go take a look...)
- [Feat] Added the "Links" tab.
- [Feat] Added the "Lyrics" tab under "Tools".
- [Feat] Added Subtitle Edit to the "Lyrics" tab under "Tools".
- [Feat] Added contributor credits for the OpenTaiko Hub to the "Credits" tab under "Information".
- [Enhancement] Some UI changes.
- [Chore] Changed the GitHub link to go to the OpenTaiko Hub's repository.
- [Chore] Moved the main OpenTaiko repository link to the "Links" tab.
- [Chore] Moved the Discord link to the "Links" tab.
- [Chore] Updated the HoF crowns to link to https://opentaiko.github.io/.
- [Chore] Fixed some typos.

## [0.1.5] - 2025-05-02 (Alpha)

- [Fix] AppImage should now be fully functional on Linux
- [Feat] Open in Explorer button next to Launch OpenTaiko to open the OpenTaiko located folder directly from the Hub
- [Chore] Replace the PeepoDrumKit Unofficial link by the repository latest release

## [0.1.4] - 2024-10-26 (Alpha)

- Hotfix

## [0.1.3] - 2024-10-26 (Alpha)


- First release with basic features
