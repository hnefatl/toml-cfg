//! # `toml-cfg`
//!
//! ## Summary
//!
//! * Crates can declare variables that can be overridden
//!     * Anything const, e.g. usize, strings, etc.
//! * (Only) The "root crate" can override these variables by including a `cfg.toml` file
//!
//! ## Config file
//!
//! This is defined ONLY in the final application or "root crate"
//!
//! ```toml
//! # a toml-cfg file
//!
//! [lib-one]
//! buffer_size = 4096
//!
//! [lib-two]
//! greeting = "Guten tag!"
//! ```
//!
//! ## In the library
//!
//! ```rust
//! // lib-one
//! #[toml_cfg::toml_config]
//! pub struct Config {
//!     #[default(32)]
//!     buffer_size: usize,
//! }
//! ```
//!
//!```rust
//! // lib-two
//! #[toml_cfg::toml_config]
//! pub struct Config {
//!     #[default("hello")]
//!     greeting: &'static str,
//! }
//!
//! ```
//!
//! ## Configuration
//!
//! With the `TOML_CFG` environment variable is set with a value containing
//! "require_cfg_present", the `toml-cfg` proc macro will panic if no valid config
//! file is found. This is indicative of either no `cfg.toml` file existing in the
//! "root project" path, or a failure to find the correct "root project" path.
//!
//! This failure could occur when NOT building with a typical `cargo build`
//! environment, including with `rust-analyzer`. This is *mostly* okay, as
//! it doesn't seem that Rust Analyzer presents this in some misleading way.
//!
//! If you *do* find a case where this occurs, please open an issue!
//!
//! ## Look at what we get!
//!
//! ```shell
//! # Print the "buffer_size" value from the `lib-one` crate.
//! # Since it has no cfg.toml, we just get the default value.
//! $ cd pkg-example/lib-one
//! $ cargo run -- quiet
//! 32
//!
//! # Print the "greeting" value from the `lib-two` crate.
//! # Since it has no cfg.toml, we just get the default value.
//! $ cd ../lib-two
//! $ cargo run --quiet
//! hello
//!
//! # Print the "buffer_size" value from `lib-one`, and "greeting"
//! # from `lib-two`. Since we HAVE defined a `cfg.toml` file, the
//! # values defined there are used instead.
//! $ cd ../application
//! $ cargo run --quiet
//! 4096
//! Guten tag!
//! ```
//!
//! ### `#[required]` fields
//!
//! ```rust
//! #[toml_cfg::toml_config]
//! pub struct Config {
//!     #[required]
//!     wifi_ssid: &'static str,
//!     // If empty assume unencrypted.
//!     #[default("")]
//!     wifi_passkey: &'static str,
//! }
//! ```
//!
//! ```toml
//! [failing-config]
//! # Oops, I forgot to set `wifi_ssid`.
//! wifi_passkey = "my_password"
//! ```
//!
//! ```console
//! $ cd ../failing-config
//! $ cargo build --quiet
//! error: custom attribute panicked
//!  --> src/lib.rs:3:1
//!   |
//! 3 | #[toml_cfg::toml_config]
//!   | ^^^^^^^^^^^^^^^^^^^^^^^^
//!   |
//!   = help: message: Field `wifi_ssid`: required but no value was provided in the config file.
//!
//! error: could not compile `failing-config` (lib) due to 1 previous error
//! ```

use heck::ToShoutySnekCase;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use syn::Expr;

#[derive(Deserialize, Clone, Debug)]
struct Config {
    #[serde(flatten)]
    crates: HashMap<String, Defn>,
}

#[derive(Deserialize, Clone, Debug, Default)]
struct Defn {
    #[serde(flatten)]
    vals: HashMap<String, toml::Value>,
}

#[proc_macro_attribute]
pub fn toml_config(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let struct_defn =
        syn::parse::<syn::ItemStruct>(item).expect("Failed to parse configuration structure!");

    let require_cfg_present = env::var("TOML_CFG").is_ok_and(|v| v.contains("require_cfg_present"));

    let cfg_path = find_root_path().map(|c| c.join("cfg.toml"));
    let maybe_cfg = cfg_path.as_ref().and_then(|c| load_crate_cfg(c));

    if require_cfg_present && maybe_cfg.is_none() {
        panic!("TOML_CFG=require_cfg_present set, but valid config not found!")
    }
    let cfg = maybe_cfg.unwrap_or_default();

    let mut struct_defn_fields = TokenStream2::new();
    let mut struct_inst_fields = TokenStream2::new();

    for field in struct_defn.fields {
        let ident = field
            .ident
            .expect("Failed to find field identifier. Don't use this on a tuple struct.");

        // Find any field attributes: `[default(...)]` or `#[required]`
        let default_attribute = field.attrs.iter().find(|a| a.path().is_ident("default"));
        let required_attribute = field.attrs.iter().find(|a| a.path().is_ident("required"));
        // Reject e.g. `#[required(0)]`, it shouldn't have any arguments.
        if let Some(e) = required_attribute.and_then(|a| a.meta.require_path_only().err()) {
            panic!(
                "Field `{}`: unexpected arguments to `#[required]`: {}",
                ident, e
            );
        }

        // Is this field provided by the config file?
        let val = match cfg.vals.get(&ident.to_string()) {
            Some(t) => {
                let t_string = t.to_string();
                syn::parse_str::<Expr>(&t_string)
                    .unwrap_or_else(|_| panic!("Field `{}`: failed to parse `{}` as a valid token!", ident, &t_string))
            }
            None => match (default_attribute, required_attribute) {
                (Some(default), None) => {
                    default.parse_args().unwrap_or_else(|e| panic!("Field `{}`: failed to parse default value: {}", ident, e))
                },
                (None, Some(_)) => panic!("Field `{}`: required but no value was provided in the config file.", ident),
                _ => panic!("Field `{}`: expected exactly one of `#[required]` or `#[default(...)]` to be provided.", ident),
            },
        };

        let ty = field.ty;
        quote! {
            pub #ident: #ty,
        }
        .to_tokens(&mut struct_defn_fields);

        quote! {
            #ident: #val,
        }
        .to_tokens(&mut struct_inst_fields);
    }

    let struct_ident = struct_defn.ident;
    let shouty_snek: TokenStream2 = struct_ident
        .to_string()
        .TO_SHOUTY_SNEK_CASE()
        .parse()
        .expect("NO NOT THE SHOUTY SNAKE");

    let hack_retrigger = if let Some(cfg_path) = cfg_path {
        let cfg_path = format!("{}", cfg_path.display());
        quote! {
            const _: &[u8] = include_bytes!(#cfg_path);
        }
    } else {
        quote! {}
    };

    quote! {
        pub struct #struct_ident {
            #struct_defn_fields
        }

        pub const #shouty_snek: #struct_ident = #struct_ident {
            #struct_inst_fields
        };

        mod toml_cfg_hack {
            #hack_retrigger
        }
    }
    .into()
}

fn load_crate_cfg(path: &Path) -> Option<Defn> {
    let contents = std::fs::read_to_string(path).ok()?;
    let parsed = toml::from_str::<Config>(&contents).ok()?;
    let name = env::var("CARGO_PKG_NAME").ok()?;
    parsed.crates.get(&name).cloned()
}

// From https://stackoverflow.com/q/60264534
fn find_root_path() -> Option<PathBuf> {
    // First we get the arguments for the rustc invocation
    let mut args = std::env::args();

    // Then we loop through them all, and find the value of "out-dir"
    let mut out_dir = None;
    while let Some(arg) = args.next() {
        if arg == "--out-dir" {
            out_dir = args.next();
        }
    }

    // Finally we clean out_dir by removing all trailing directories, until it ends with target
    let mut out_dir = PathBuf::from(out_dir?);
    while !out_dir.ends_with("target") {
        if !out_dir.pop() {
            // We ran out of directories...
            return None;
        }
    }

    out_dir.pop();

    Some(out_dir)
}
