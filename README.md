# `toml-cfg`

Rough ideas:

* Crates can declare variables that can be overridden
    * Anything const, e.g. usize, strings, etc.
* (Only) The "root crate" can override these variables by including a `cfg.toml` file
* Variables can either have default values (`#[default(...)]`) or fail at compile-time if not configured (`#[required]`)

## Config file

```toml
# a toml-cfg file

[lib-one]
buffer_size = 4096

[lib-two]
greeting = "Guten Tag!"
```

### In the library

```rust
// lib-one
#[toml_cfg::toml_config]
pub struct Config {
    #[default(32)]
    buffer_size: usize,
}

// lib-two
#[toml_cfg::toml_config]
pub struct Config {
    #[default("hello")]
    greeting: &'static str,
}
```

### Look at what we get!

```console
# Print the "buffer_size" value from the `lib-one` crate.
# Since it has no cfg.toml, we just get the default value.
$ cd pkg-example/lib-one
$ cargo run --quiet
32

# Print the "greeting" value from the `lib-two` crate.
# Since it has no cfg.toml, we just get the default value.
$ cd ../lib-two
$ cargo run --quiet
hello

# Print the "buffer_size" value from `lib-one`, and "greeting"
# from `lib-two`. Since we HAVE defined a `cfg.toml` file, the
# values defined there are used instead.
$ cd ../application
$ cargo run --quiet
4096
Guten Tag!
```

### `#[required]` fields

```rust
#[toml_cfg::toml_config]
pub struct Config {
    #[required]
    wifi_ssid: &'static str,
    // If empty assume unencrypted.
    #[default("")]
    wifi_passkey: &'static str,
}
```

```toml
[failing-config]
# Oops, I forgot to set `wifi_ssid`.
wifi_passkey = "my_password"
```

```console
$ cd ../failing-config
$ cargo build --quiet
error: custom attribute panicked
 --> src/lib.rs:3:1
  |
3 | #[toml_cfg::toml_config]
  | ^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = help: message: Field `wifi_ssid`: required but no value was provided in the config file.

error: could not compile `failing-config` (lib) due to 1 previous error
```
