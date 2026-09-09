# OpenTaiko-Hub
Launcher, updater and asset manager for OpenTaiko.

## Credits
### Contributor credits
> * [BeaniCraft/The 0.1.6 and 0.1.7 updates](https://github.com/BeaniCraft)
> * [IID/Revisions for Chinese (Traditional)](https://github.com/IepIweidieng)
> * [Taichenko/Revisions for Chinese (Simplified)](https://github.com/Taichenko)

---

### Theme credits
#### OpenTaiko Hub themes
> * [BeaniCraft/"Gleaming Sky", "888", "Deceiver", "Onyx", "Pearl", and "OpenTaiko-Kun".](https://github.com/BeaniCraft)

#### Skeleton preset themes
The Skeleton preset themes are themes provided by [Skeleton UI.](https://www.skeleton.dev/ ) (Specifically Skeleton v2.)
<br>You can find info about the provided Skeleton themes [here.](https://www.skeleton.dev/docs/design/themes)

## How to Test Locally

### First Time: Environment Set up (Windows)

Follow the instruction of <https://tauri.app/start/prerequisites/#windows>.

Warning: Using WSL (Windows Subsystem for Linux) to test the project will not work because the path for Windows version of OpenTaiko is different. Installing natively on the Windows side is necessary.

### Build and Launch Dev Server

Open terminal, `cd` into the OpenTaiko-Hub directory, run `npm install` to install or update the dependency packages.

Then run `npm run tauri dev`, and then wait the project to build and the OpenTaiko-Hub window to boot.

The frontend is TypeScript: run `npm run check` to type-check it (the Rust backend is checked with `cargo check` in `src-tauri`).

## How to Build Locally

Use the release build from GitHub action is usually enough. However, you can still run `npm run tauri build` to build the build.
