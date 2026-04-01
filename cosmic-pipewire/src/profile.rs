// Copyright 2025 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use std::ffi::c_int;

use crate::{
    Availability,
    spa_utils::{array_from_pod, string_from_pod},
};
use libspa::pod::Pod;

#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub index: i32,
    pub priority: i32,
    pub available: Availability,
    pub name: String,
    pub description: String,
    pub classes: Vec<ProfileClass>,
}

#[derive(Clone, Debug)]
pub enum ProfileClass {
    AudioSink { card_profile_devices: Vec<i32> },
    AudioSource { card_profile_devices: Vec<i32> },
}

impl Profile {
    pub fn from_pod(pod: &Pod) -> Option<Self> {
        let mut index = 0;
        let mut priority = 0;
        let mut available = Availability::Unknown;
        let mut name = String::new();
        let mut description = String::new();
        let mut classes = Vec::new();

        let profile = pod.as_object().ok()?;

        for prop in profile.props() {
            match prop.key().0 {
                libspa_sys::SPA_PARAM_PROFILE_index => index = prop.value().get_int().ok()?,
                libspa_sys::SPA_PARAM_PROFILE_priority => priority = prop.value().get_int().ok()?,
                libspa_sys::SPA_PARAM_PROFILE_available => {
                    available = match prop.value().get_id().unwrap().0 {
                        libspa_sys::SPA_PARAM_AVAILABILITY_no => Availability::No,
                        libspa_sys::SPA_PARAM_AVAILABILITY_yes => Availability::Yes,
                        _ => Availability::Unknown,
                    };
                }
                libspa_sys::SPA_PARAM_PROFILE_name => name = string_from_pod(prop.value())?,
                libspa_sys::SPA_PARAM_PROFILE_description => {
                    description = string_from_pod(prop.value())?;
                }
                libspa_sys::SPA_PARAM_PROFILE_classes => {
                    let profile_classes = prop.value().as_struct().unwrap();

                    for class in profile_classes.fields() {
                        let Ok(class) = class.as_struct() else {
                            continue;
                        };

                        let mut fields = class.fields();
                        let Some(class_name) = fields.next().and_then(string_from_pod) else {
                            continue;
                        };

                        fields.next();

                        while let Some((key, value)) = fields.next().zip(fields.next()) {
                            if let Some("card.profile.devices") = string_from_pod(key).as_deref() {
                                if let Some(card_profile_devices) =
                                    unsafe { array_from_pod::<c_int>(value) }
                                {
                                    classes.push(match class_name.as_str() {
                                        "Audio/Sink" => ProfileClass::AudioSink {
                                            card_profile_devices,
                                        },
                                        _ => ProfileClass::AudioSource {
                                            card_profile_devices,
                                        },
                                    });
                                }
                            }
                        }
                    }
                }
                _ => (),
            }
        }

        Some(Self {
            index,
            priority,
            available,
            name,
            description,
            classes,
        })
    }
}
