# search-tui

A terminal UI for the metadata search engine, built with [ratatui](https://ratatui.rs). Fans out queries to DuckDuckGo, Brave, and Startpage concurrently and displays RRF-ranked results you can browse and open directly from the terminal.

## Running

From the workspace root:

```bash
cargo run -p search-tui
# or after installing:
stx
```

## Usage

The TUI starts in search mode. Type a query and press `Enter` — results load in the background while the interface stays responsive.

### Keybinds

| Key | Action |
| --- | --- |
| `Enter` | Submit search query |
| `j` / `↓` | Next result |
| `k` / `↑` | Previous result |
| `g` | Jump to first result |
| `G` | Jump to last result |
| `l` or `Enter` | Open selected URL in browser |
| `h` or `/` | Back to search bar |
| `q` / `Esc` | Quit |
| `Ctrl-c` | Force quit from any mode |

## Layout

```
┌ Search ──────────────────────────────────────────┐
│ rust programming                                  │
├ Results (10) ─────────────────────────────────────┤
│ ▶ #1 Rust Programming Language  [duckduckgo, brave, startpage] │
│      https://rust-lang.org                        │
│      A language empowering everyone to build...   │
│                                                   │
│   #2 The Rust Book  [duckduckgo, startpage]       │
│      https://doc.rust-lang.org/book               │
│      ...                                          │
├───────────────────────────────────────────────────┤
│  ↑↓ / jk: navigate   l/Enter: open   h/: search  │
└───────────────────────────────────────────────────┘
```

Results show the RRF score-based rank, the engines that returned them (more engines = higher rank), and a snippet. Press `l` or `Enter` on any result to open it in your default browser.
