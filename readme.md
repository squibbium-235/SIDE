# SIDE

SIDE is a small desktop code editor built with Rust and Dioxus Desktop. It focuses on being lightweight, hackable, and easy to run while still having the basics you actually need.

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
    - Compile with ```bash cargo run```

  - Typical Structire
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
  - `src/syntax.rs` - `.sidel` loading, parsing, highlighting engine
  - `syntax/` - syntax definitions (`*.sidel`)
  - `assets/fonts/` - bundled fonts (JetBrains Mono)
  - `current.ver` - contains the latest version number, polls the github on every launch to check for updates

## Requirements

### Runtime (Windows)
- Microsoft WebView2 Runtime is required by Dioxus Desktop on Windows. Many machines already have it, but “isolated” systems might not.
- If you build with the MSVC toolchain, the target machine may also need the Visual C++ runtime.

### Build
- Rust toolchain (stable)
- Recommended: latest stable `cargo`

## Build and run

From the `ide/` directory:

```bash
cargo run
```