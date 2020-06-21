# Darkest Dungeon Mod Bundler

This small program is created for everyone who, like me, is a little upset with Darkest Dungeon modding scheme. How many times did you see that two mods you like are incompatible, because they overwrite the same files in different places? This tool can possibly help you.

## How it works

The idea is extremely simple and is well-known to probably any programmer. We just treat every mod not as a replacement for the original files, but as a *patch*, i.e. the list of *changes*. Then, if this changes are non-conflicting, i.e. if they are performed in different places, we can merge them into one large patch, apply this patch to the vanilla/DLC files and store the result as a new mod, which can be used as a replacement for the original ones.

## Disclaimer

This program is written as a personal tool. The current release is what I cat call the "minimal viable product", with heavy accent on "minimal". This code is still fairly inefficient, it consumes a lot of memory and can even crash due to insufficient RAM, if the mod contains large text files (most notably, if it changes some of the vanilla string tables). There is no GUI, only TUI, and even this is not very polished. So, if you find something you think might be improved, feel free to open an issue - I'll see what I can do.

Also, if you experience unexpected crush or some other error, run the executable in debug mode (`darkest_dungeon_mod_bundler --debug`) and send me the `log` file from the executable directory, along with the error description. I'll try to find a root cause.

## Known limitations

There are several limitations in current version:
- The program can only work with Steam version of Darkest Dungeon and reads only mods downloaded from Steam Workshop.
- If several mods add content after the same line of original file, the bundler will exit with error.
- If some mod adds content to the beginning of text file, this content will be added after the first line of the file.

These limitations may be fixed in the future versions, although I can't promise anything, since some changes might require major rewrite.

## Development notes

This program is written in [Rust](http://www.rust-lang.org). To build it, you have to first install Rust and Cargo; the easiest way is by using [rustup](https://rustup.rs/). After that, use:
```
cargo run
```
to run the program collecting only errors, or
```
cargo run -- --debug
```
to run it collecting the full logs.