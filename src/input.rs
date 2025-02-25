// Copyright 2025 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use anyhow::Context;
use cosmic_comp_config::XkbConfig;
use cosmic_config::{ConfigGet, ConfigSet};
use xkb_data::KeyboardLayout;

const COSMIC_COMP_CONFIG: &str = "com.system76.CosmicComp";
const COSMIC_COMP_CONFIG_VERSION: u64 = 1;
const XKB_CONFIG_KEY: &str = "xkb_config";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ActiveLayout {
    layout: String,
    description: String,
    variant: String,
}

/// Take the current xkb config and switch the active input source.
pub fn source_switch() -> anyhow::Result<()> {
    let config =
        cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION).unwrap();

    let mut xkb = config.get(XKB_CONFIG_KEY).unwrap_or_else(|why| {
        if why.is_err() {
            eprintln!("failed to read config '{}': {}", XKB_CONFIG_KEY, why);
        }

        XkbConfig::default()
    });

    let keyboard_layouts =
        xkb_data::all_keyboard_layouts().context("could not get keyboard layouts data")?;

    let mut active_layouts = xkb_active_layouts(&xkb, keyboard_layouts.layouts());

    let prev_layout = active_layouts.remove(0);
    active_layouts.push(prev_layout);

    update_xkb_config(
        &config,
        &mut xkb,
        &mut active_layouts
            .iter()
            .map(|layout| (layout.layout.as_str(), layout.variant.as_str())),
    )
    .context("Failed to set config 'xkb_config'")
}

fn xkb_active_layouts(xkb: &XkbConfig, keyboard_layouts: &[KeyboardLayout]) -> Vec<ActiveLayout> {
    let mut active_layouts = Vec::new();

    let layouts = xkb.layout.split_terminator(',');

    let variants = xkb
        .variant
        .split_terminator(',')
        .chain(std::iter::repeat(""));

    'outer: for (layout, variant) in layouts.zip(variants) {
        for xkb_layout in keyboard_layouts {
            if layout != xkb_layout.name() {
                continue;
            }

            if variant.is_empty() {
                let active_layout = ActiveLayout {
                    description: xkb_layout.description().to_owned(),
                    layout: layout.to_owned(),
                    variant: variant.to_owned(),
                };

                active_layouts.push(active_layout);
                continue 'outer;
            }

            let Some(xkb_variants) = xkb_layout.variants() else {
                continue;
            };

            for xkb_variant in xkb_variants {
                if variant != xkb_variant.name() {
                    continue;
                }

                let active_layout = ActiveLayout {
                    description: xkb_variant.description().to_owned(),
                    layout: layout.to_owned(),
                    variant: variant.to_owned(),
                };

                active_layouts.push(active_layout);
                continue 'outer;
            }
        }
    }

    active_layouts
}

fn update_xkb_config(
    config: &cosmic_config::Config,
    xkb: &mut XkbConfig,
    active_layouts: &mut dyn Iterator<Item = (&str, &str)>,
) -> Result<(), cosmic_config::Error> {
    let mut new_layout = String::new();
    let mut new_variant = String::new();

    for (locale, variant) in active_layouts {
        new_layout.push_str(locale);
        new_layout.push(',');
        new_variant.push_str(variant);
        new_variant.push(',');
    }

    let _excess_comma = new_layout.pop();
    let _excess_comma = new_variant.pop();

    xkb.layout = new_layout;
    xkb.variant = new_variant;

    config.set("xkb_config", xkb)
}
