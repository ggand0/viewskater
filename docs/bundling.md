# Building and Packaging ViewSkater

This guide explains how to build and package ViewSkater for different operating systems.

## Linux

To build and package ViewSkater as an AppImage:

```sh
cargo install cargo-appimage
cargo appimage
```

This will generate a standalone `.AppImage` in the `target/appimage/` directory.


## Windows

To build the release binary:

```sh
cargo build --release
```
This will generate `viewskater.exe` in the `target/release/` directory.
You can wrap it in an installer using tools like [Inno Setup](https://github.com/jrsoftware/issrc) if needed.


## macOS

To bundle the `.app` and create a `.dmg`:

```sh
cargo bundle --release
```

Then:

```sh
cd target/release/bundle/osx
hdiutil create -volname "ViewSkater" -srcfolder "ViewSkater.app" -ov -format UDZO "view_skater.dmg"
```

This will generate a `.dmg` file suitable for distribution.


## Notes

- Tested on Rust 1.85.1
- macOS bundling uses [`cargo-bundle`](https://github.com/burtonageo/cargo-bundle)
- Linux packaging uses [`cargo-appimage`](https://github.com/linuxwolf/cargo-appimage)