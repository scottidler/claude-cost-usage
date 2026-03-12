# claude-cost-usage
Rust CLI for reading Claude Code JSONL session files and computing daily/weekly cost summaries.

## Statusline Integration

Power your shell statusline with live Claude spend data. The example below shows `statusline.sh` using `ccu` to display Monthly, Weekly, Today, and Session costs:

![statusline example](assets/statusline.png)

```bash
# Example statusline.sh snippet
monthly=$(ccu monthly --total -m 1)
weekly=$(ccu weekly --total -w 1)
today=$(ccu today --total)
session=$(ccu session current --total)

echo "Claude: M\$$monthly W\$$weekly D\$$today S\$$session"
```

### Statusline Setup

`statusline.sh` lives in `~/.claude/` and runs periodically to update your shell prompt. You can:

- Write your own custom `statusline.sh` that calls `ccu` for cost data
- Use an open-source option like [Owloops/claude-powerline](https://github.com/Owloops/claude-powerline)
- Have Claude Code itself write a custom statusline for you

`ccu` provides the cost data - `statusline.sh` decides how to display it. The `--total` flag outputs a plain number suitable for embedding in status strings.

## Installation

### Quick Install

```bash
curl -fsSL https://raw.githubusercontent.com/scottidler/claude-cost-usage/main/install.sh | bash
```

Options:
```bash
# Install to a custom directory
curl -fsSL https://raw.githubusercontent.com/scottidler/claude-cost-usage/main/install.sh | bash -s -- --to ~/bin

# Install a specific version
curl -fsSL https://raw.githubusercontent.com/scottidler/claude-cost-usage/main/install.sh | bash -s -- --version v0.3.0
```

### From Source

```bash
cargo install --git https://github.com/scottidler/claude-cost-usage
```

## Usage

```bash
# Today's cost
ccu

# Yesterday's cost
ccu yesterday

# Last 7 days (daily breakdown)
ccu daily

# Weekly summary (last 4 weeks)
ccu weekly

# Monthly summary (last 12 months)
ccu monthly

# Plain cost number (for scripts/statuslines)
ccu today --total       # e.g. "14.23"
ccu monthly --total -m 1

# JSON output
ccu today --json

# Verbose (per-session breakdown)
ccu today -v

# With graphs
ccu daily -g
ccu weekly -g
```

## Pricing

`ccu` ships with embedded pricing for all current Claude models, compiled into the binary. No config file or network connection is needed for normal operation.

### Checking for stale pricing

```bash
# Check if the embedded pricing might be outdated
ccu pricing --check
```

Exit codes: `0` = up to date, `1` = pricing page has changed (may be stale), `2` = fetch failed.

### Viewing current pricing

```bash
ccu pricing --show
```

### Custom/enterprise rates

Create or edit `~/.config/ccu/ccu.yml` to override specific model prices:

```yaml
pricing:
  claude-opus-4-6:
    input_per_mtok: 4.50
    output_per_mtok: 22.50
    cache_5m_write_per_mtok: 5.63
    cache_1h_write_per_mtok: 9.0
    cache_read_per_mtok: 0.45
```

Config pricing overrides are merged on top of the embedded defaults. Models not in your config use the embedded values.

### Updating pricing (developers)

When Anthropic changes their pricing, run:

```bash
bin/update
```

This fetches the live pricing page, parses it deterministically, and regenerates `data/pricing.yml`. Review the diff, commit, and cut a release.

## Version Reporting

The `ccu` binary supports `--version` and `-v` flags:

```
$ ccu --version
ccu v0.3.0
```

- The version is driven by the latest annotated git tag and the output of `git describe`.
- If the current commit is exactly at a tag (e.g., `v0.3.0`), the version will be `ccu v0.3.0`.
- If there are additional commits, it will show something like `ccu v0.3.0-3-gabcdef`.

## Release & Versioning Process

1. **Bump the version in `Cargo.toml`** to the new release version (e.g., `0.4.0`).
2. **Commit** the change.
3. **Tag** the commit with an annotated tag: `git tag -a v0.4.0 -m "Release v0.4.0"`.
4. **Push** the tag: `git push --tags`.
5. **Build** the binary. The version will be embedded from the tag and `git describe`.
6. **Create a GitHub Release** and upload the binary. The version in the binary will match the release tag.

> If the version in `Cargo.toml` does not match the latest tag, a warning will be printed at build time.
