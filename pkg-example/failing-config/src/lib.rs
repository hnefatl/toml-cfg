#![no_std]

#[toml_cfg::toml_config]
pub struct Config {
    #[required]
    wifi_ssid: &'static str,
    // If empty assume unencrypted.
    #[default("")]
    wifi_passkey: &'static str,
}
