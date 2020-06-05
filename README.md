# temporal-lens

## **NOTICE: WORK IN PROGRESS**

Temporal Lens is **still under development** and should not be used for now.

## What is Temporal Lens?

Temporal Lens is a telemetry infrastructure for Rust. Inspired by wolfpld's [tracy](https://github.com/wolfpld/tracy), it is designed essentially for game
engine profiling. It consists in three projects :

 * [temporal-lens](https://github.com/temporal-lens-team/temporal-lens), a dependency you add to the Rust project you want to profile
 * [temporal-lens-server](https://github.com/temporal-lens-team/temporal-lens-server), an application that receives data from the `temporal-lens` library via IPC mechanisms, stores it internally, and exposes it to clients such as `temporal-lens-vscode` through a RESTful API
 * [temporal-lens-vscode](https://github.com/temporal-lens-team/temporal-lens-vscode), a Visual Studio Code extension which acquires the data recorded by `temporal-lens-server` and displays it as user-friendly graphs directly inside a vscode tab

## What is this specific git repository about?

`temporal-lens` is the library you should plug into the Rust project you want to profile. It provides easy-to-use macros and functions which allows you to
carefuly define which part of the software you'd like to profile. As the said macros and functions might have a small perfromance impact on your software,
one of this library's goal is to provide the developer with the ability to easily disable all `temporal-lens` functions without the need of modifying your
code.

On the technical side of things, the `temporal-lens` library's only job is to gather time differences as well as some context information and send everything
to `temporal-lens-server` with the help of IPCs.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
