// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use calloop_wayland_source::WaylandSource;
use cctk::{
    cosmic_protocols::keyboard_layout::v1::client::zcosmic_keyboard_layout_v1::ZcosmicKeyboardLayoutV1,
    keyboard_layout::{KeyboardLayoutHandler, KeyboardLayoutState},
    sctk::{
        self,
        registry::{ProvidesRegistryState, RegistryState},
        seat::{Capability, SeatHandler, SeatState},
    },
    wayland_client::{
        Connection, QueueHandle, delegate_noop,
        globals::registry_queue_init,
        protocol::{wl_keyboard, wl_seat},
    },
};
use cosmic_comp_config::XkbConfig;
use cosmic_config::ConfigGet;
use std::thread;

pub enum Cmd {
    InputSourceSwitch,
}

pub fn run() -> calloop::channel::Sender<Cmd> {
    let conn = Connection::connect_to_env().unwrap();
    let (sender, channel) = calloop::channel::channel();
    thread::spawn(move || thread(conn, channel));
    sender
}

const COSMIC_COMP_CONFIG: &str = "com.system76.CosmicComp";
const COSMIC_COMP_CONFIG_VERSION: u64 = 1;
const XKB_CONFIG_KEY: &str = "xkb_config";

fn xkb_config() -> Option<XkbConfig> {
    let config =
        cosmic_config::Config::new(COSMIC_COMP_CONFIG, COSMIC_COMP_CONFIG_VERSION).unwrap();

    match config.get(XKB_CONFIG_KEY) {
        Ok(xkb) => Some(xkb),
        Err(why) => {
            if why.is_err() {
                log::error!("failed to read config '{}': {}", XKB_CONFIG_KEY, why);
                None
            } else {
                Some(XkbConfig::default())
            }
        }
    }
}

struct Keyboard {
    seat: wl_seat::WlSeat,
    keyboard_layout: ZcosmicKeyboardLayoutV1,
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        self.keyboard_layout.destroy();
    }
}

struct AppData {
    seat_state: SeatState,
    registry_state: RegistryState,
    keyboard_layout_state: KeyboardLayoutState,
    running: bool,
    keyboard: Option<Keyboard>,
    current_layout: u32,
}

impl AppData {
    fn input_source_switch(&mut self) {
        if let Some(keyboard) = &self.keyboard {
            if let Some(xkb) = xkb_config() {
                let count = xkb.layout.split_terminator(',').count();

                let group = (self.current_layout + 1) % count as u32;
                keyboard.keyboard_layout.set_group(group);
                self.current_layout = group;
            }
        }
    }
}

impl KeyboardLayoutHandler for AppData {
    fn group(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _keyboard_layout: &ZcosmicKeyboardLayoutV1,
        group: u32,
    ) {
        self.current_layout = group;
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers![SeatState,];
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let keyboard = seat.get_keyboard(qh, ());
            let keyboard_layout = self
                .keyboard_layout_state
                .get_keyboard_layout(&keyboard, qh);
            keyboard.release();

            if let Some(keyboard_layout) = keyboard_layout {
                self.keyboard = Some(Keyboard {
                    seat,
                    keyboard_layout,
                });
            } else {
                keyboard.release();
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        _capability: Capability,
    ) {
        self.keyboard.take_if(|x| x.seat == seat);
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.keyboard.take_if(|x| x.seat == seat);
    }
}

fn thread(conn: Connection, channel: calloop::channel::Channel<Cmd>) {
    let (globals, event_queue) = registry_queue_init(&conn).unwrap();
    let qh: QueueHandle<AppData> = event_queue.handle();
    let registry_state = RegistryState::new(&globals);
    let seat_state = SeatState::new(&globals, &qh);
    let keyboard_layout_state = KeyboardLayoutState::new(&registry_state, &qh);

    let mut event_loop = calloop::EventLoop::try_new().unwrap();
    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .unwrap();
    event_loop
        .handle()
        .insert_source(channel, |event, _, app_data| match event {
            calloop::channel::Event::Msg(cmd) => match cmd {
                Cmd::InputSourceSwitch => app_data.input_source_switch(),
            },
            calloop::channel::Event::Closed => {
                app_data.running = false;
            }
        })
        .unwrap();

    let mut app_data = AppData {
        seat_state,
        registry_state,
        keyboard_layout_state,
        running: true,
        keyboard: None,
        current_layout: 0,
    };
    while app_data.running {
        event_loop.dispatch(None, &mut app_data).unwrap();
    }
}

sctk::delegate_registry!(AppData);
sctk::delegate_seat!(AppData);
cctk::delegate_keyboard_layout!(AppData);
delegate_noop!(AppData: ignore wl_keyboard::WlKeyboard);
