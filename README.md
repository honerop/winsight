# Winsight (Window Time Tracker)

A cross-platform **window focus tracker** that records how much time you spend on each application.
Built in Rust for performance and portability.

---

## Features
- **Cross-platform**: Supports Linux (Sway/X11), macOS, and Windows.
- **Lightweight**: Minimal dependencies, no GUI, and low resource usage.
- **Persistent**: Saves window focus durations to `~/.local/share/window-time-tracker/durations.tsv`.
- **Real-time**: Logs focus changes as they happen.

---

## Installation

### From Source
1. Ensure [Rust](https://rust-lang.org) is installed (`rustc` and `cargo`).
2. Clone this repository:
   ```sh
   git clone https://github.com/your-username/winsight.git
   cd winsight
   ```
3. Build and run:
   ```sh
   cargo run --release
   ```

### Nix (Optional)
If using Nix, a `flake.nix` is provided for development:
```sh
nix develop
cargo run
```

---

## Usage
Run the binary to start tracking:
```sh
cargo run
```
- Focus changes are logged to stdout (e.g., `-> firefox`).
- Data is saved to `~/.local/share/window-time-tracker/durations.tsv` in the format:
  ```
  <window_name>	<duration_seconds>
  ```

---

## Configuration
### Backends
- **Linux**: Auto-detects between Sway (Wayland) and X11 backends.
- **macOS**: Uses native Objective-C APIs.
- **Windows**: Uses Win32 APIs.

To enable a specific backend, uncomment the relevant dependencies in `Cargo.toml`:
```toml
[target.'cfg(target_os = "linux")'.dependencies]
swayipc = "3"      # For Sway (Wayland)
x11rb = "0.13"     # For X11
```

---

## Data Storage
- **Location**: `~/.local/share/window-time-tracker/durations.tsv`
- **Format**: Tab-separated values (window name → total seconds).
- **Example**:
  ```
  firefox	3600
  vscode	7200
  ```

---

## Development
### Project Structure
```
src/
  backend/
    linux.rs   # Linux-specific backend (Sway/X11)
    macos.rs   # macOS backend
    mod.rs     # Backend trait and detection logic
    windows.rs # Windows backend
  main.rs      # Core logic (event handling, storage)
```

### Adding a New Backend
1. Implement the `FocusBackend` trait in a new module (e.g., `src/backend/wayland.rs`).
2. Update `detect_backend()` in `mod.rs` to include the new backend.
3. Add dependencies to `Cargo.toml` under the appropriate `target_os` section.

---

## License
MIT OR Apache-2.0 (dual-licensed, same as Rust).