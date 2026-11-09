# Odyssey Coins

Lists the kingdom coins a Super Mario Odyssey save has not collected yet.

A built copy is [`Odyssey.Coins.exe`](https://github.com/oiuy-retro/odyssey-coins/releases/latest/download/Odyssey.Coins.exe) on the [latest release](https://github.com/oiuy-retro/odyssey-coins/releases/latest).

This program does not include the game, its levels, its text, or a coin location list. You supply your own save. Places are read from an extracted copy of the game that stays on your computer.

## What it does

- Reads `File2.bin` and counts collected and missing kingdom coins.
- With an extracted game folder, groups the missing coins by kingdom.
- Shows each missing coin's stage, object id, position, and the nearest checkpoint in that stage.
- Checkpoint names come from `StageMessage.szs` when that file is in the folder you pass. Otherwise the checkpoint's object id is shown.

## Run

1. Copy `File2.bin` from your Eden save for Super Mario Odyssey (title `0100000000010000`). That file is the current slot.
2. To name kingdoms and show places, extract the game's romfs yourself. This repo does not include those files and does not download them.
3. Run:

```bat
"Odyssey Coins.exe" --save File2.bin --content path\to\romfs
```

The romfs folder is expected to contain:

- `SystemData/WorldList.szs` (required for kingdom names)
- `StageData/<stage>Map.szs` (positions)
- `LocalizedData/USen/MessageData/StageMessage.szs` (checkpoint names)

Without `--content`, the program only prints the missing coin ids from the save.

```bat
"Odyssey Coins.exe" --help
```

## Build

This crate targets `x86_64-pc-windows-gnu` (`rust-toolchain.toml`).

```bat
cargo build --release
copy target\release\odyssey-coins.exe "Odyssey Coins.exe"
```

## License

[MIT](LICENSE).
