# sepiida

An adaptive theming engine for the Windows shell. sepiida reads your current
desktop wallpaper, derives a colour palette from it, and applies it to Windows
itself: accent colour, light/dark mode, taskbar, Start menu and File Explorer.
Ordinary applications are left alone.

It works with two wallpaper sources:

- the normal Windows wallpaper (picture, slideshow or Windows Spotlight)
- [Wallpaper Engine](https://www.wallpaperengine.io/), which is optional

By default sepiida follows Wallpaper Engine when it is running and showing a
wallpaper, and the Windows wallpaper otherwise.

> **Status: early development.** `sepiida apply` sets the Windows accent
> colour from the current wallpaper; nothing runs in the background yet.
> Later versions load a DLL into Windows shell processes; until that is
> stable, expect it to be able to crash or restart Explorer.

## Usage

```
sepiida apply                    set the accent colour from the current wallpaper
sepiida apply --dry-run          show what would change without changing it
sepiida apply --source windows   ignore Wallpaper Engine (or: wallpaper-engine, auto)
sepiida apply --mode dark        also switch Windows to dark (or light) mode
sepiida palette image.png        print the palette for an image
```

`apply` writes the accent colour to the current user's registry settings and
turns off "Automatically pick an accent colour from my background", which
would otherwise overwrite it. Light/dark mode is left alone unless `--mode`
is given.

## Building

Development happens on Linux with [Nix](https://nixos.org/) and cross-compiles
to Windows:

```sh
nix develop
cargo test --workspace
cargo run -p sepiida -- palette path/to/image.png
cargo xwin build --workspace --release --target x86_64-pc-windows-msvc
```

The first cross-build downloads the Microsoft CRT and Windows SDK through
`cargo-xwin`, which means accepting Microsoft's licence terms for them.

On Windows, install Rust with `rustup` and run `cargo build --release`.

Every push to `main` also builds a Windows artifact (`sepiida-windows-x64`) in
GitHub Actions.

## Licence

GPL-3.0-only. See [LICENSE](LICENSE).
