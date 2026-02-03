## SIDE NOW COMPILES FOR LINUX

- SIDE now compiles and runs on linux, there won't be any binary releases until I can work out how to make appimages or .deb and .rpm files, instructions for compilation [here](#build)

# SIDE

SIDE is a small desktop code editor built with Rust and Dioxus Desktop. It focuses on being lightweight, hackable, and easy to run while still having the basics.

## Features

- Desktop UI built with Dioxus Desktop
- Open and save files using native file dialogs
- Tabbed editing
- Sidebar file view (project browsing)
- Syntax highlighting driven by simple `.sidel` files
  - `.sidel` syntax files are embedded into the binary on compilation

## sidel Files

- sidel files are language definition files that live within `ide/syntax/<language_name>.sidel`
  - They use TOML formatting and Regex expressions, SIDE has syntax highlighting for sidel files to make editing easier
  - To add a new language:
    - create a sidel file in `ide/syntax/` with the name `<language>.sidel`
    - Fill in the TOML file
    - Update `ide/syntax/manifest.toml` with the new language
      - `manifest.toml` section example
        ```toml
        [[language]]
        name = "<sidel file name>"
        extensions = ["<file extention>", "<file extention 2>"]
        ```
    - Compile with ```cargo run```

  - Typical Structure
    ```toml
    default_color = "#D4D4D4"

    [[rule]]
    name = "Keyword"
    pattern = "\\b(fn|let|pub|struct)\\b"
    color = "#C586C0"
    priority = 10
    ```


## Project structure

- `ide/`
  - `src/main.rs` - UI, tabs, editor logic
  - `src/syntax.rs` - `.sidel` loading, parsing and highlighting
  - `syntax/` - syntax definitions (`*.sidel`)
  - `syntax/manifest.toml` - contains language file extensions and names
  - `assets/fonts/` - bundled fonts (JetBrains Mono)
  - `current.ver` - contains the latest version number, polls the github on every launch to check for updates

## Requirements

- Windows 10 and up (only compiles on windows for now)
- MS WebView2 Runtime
- Whatever windows requires

### Runtime (Windows)
- Microsoft WebView2 Runtime is required by Dioxus Desktop on Windows. If your computer has the chromium version of edge, it has this.
- If you build with the MSVC toolchain, the target machine may also need the Visual C++ runtime.

### Build
- Rust 2024
- Recommended: latest stable cargo

- Windows
  - Install rust 2024 latest
  - navigate to the `ide` directory
  - run `cargo build --release`

- Linux (and any *nix hopefully)
  - Debian / Ubuntu
    - ```bash
      sudo apt-get update
      sudo apt-get install -y pkg-config libwayland-dev libxkbcommon-dev libgtk-3-dev libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libegl1-mesa libgl1-mesa-dri mesa-vulkan-drivers`
      ```
      - (These were the minimum requirements I needed to get it to build with WSL)

  - Arch / Manjaro
    - ```bash
      sudo pacman -Syu --needed pkgconf wayland libxkbcommon gtk3 webkit2gtk-4.1 xdotool openssl mesa
      ```
      - (Untested)
    
  - Fedora / RHEL
    - Fedora
      - ```bash
        sudo dnf install -y pkgconf-pkg-config wayland-devel libxkbcommon-devel gtk3-devel webkit2gtk4.1-devel xdotool-devel openssl-devel mesa-libEGL mesa-dri-drivers mesa-vulkan-drivers
        ```
    - RHEL, Rocky and Alma
      - Ensure CRB is enabled for `-devel` packages
        - `sudo dnf config-manager --set-enabled crb`
      - `sudo dnf install -y pkgconf-pkg-config wayland-devel libxkbcommon-devel gtk3-devel webkit2gtk4.1-devel xdotool-devel openssl-devel mesa-libEGL mesa-dri-drivers mesa-vulkan-drivers`

  - openSUSE
    - `sudo zypper install -y pkg-config wayland-devel libxkbcommon-devel gtk3-devel webkit2gtk-4_1-devel xdotool-devel libopenssl-devel Mesa-libEGL1 Mesa-dri`

  - Alpine
    - `sudo apk add pkgconf wayland-dev libxkbcommon-dev gtk+3.0-dev webkit2gtk-dev xdotool-dev openssl-dev mesa-egl mesa-dri-gallium`
     - (UNTESTED! Some rust crates may fail because alpine uses musl)

  - NixOS
    - `nix-shell -p pkg-config wayland libxkbcommon gtk3 webkitgtk_4_1 xdotool openssl mesa`
    - OR add those packages to the thingy file

  - Void
    - `sudo xbps-install -S pkg-config wayland-devel libxkbcommon-devel gtk+3-devel webkit2gtk-devel xdotool-devel openssl-devel mesa mesa-dri`

  - Solus
    - `sudo eopkg install -y pkg-config wayland-devel libxkbcommon-devel gtk3-devel webkitgtk-devel xdotool-devel openssl-devel mesa`

  - Gentoo
    - `sudo emerge --ask dev-util/pkgconf dev-libs/wayland x11-libs/libxkbcommon x11-libs/gtk+:3 net-libs/webkit-gtk x11-misc/xdotool dev-libs/openssl media-libs/mesa`

  - Slackware
    - `sbopkg -i pkg-config wayland libxkbcommon gtk+3 webkit2gtk xdotool openssl mesa`

  - ALL
    - `cargo build --release`

  - If these fail, make an issue and I will attempt to fix and update the readme

## Build and run

- From the `ide/` directory:

  - Build
    - `cargo build --release`

  - Run
    - `cargo run --release`