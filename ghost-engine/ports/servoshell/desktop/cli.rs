/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::{env, panic};

use log::{info, warn};

use crate::desktop::app::App;
use crate::desktop::event_loop::ServoShellEventLoop;
use crate::panic_hook;
use crate::prefs::{ArgumentParsingResult, parse_command_line_arguments};

const FORCE_HEADLESS_ENV: &str = "SERVOSHELL_FORCE_HEADLESS";

fn force_headless_requested() -> bool {
    env::var(FORCE_HEADLESS_ENV).is_ok_and(|value| {
        !matches!(value.trim().to_ascii_lowercase().as_str(), "" | "0" | "false" | "no" | "off")
    })
}

pub fn main() {
    crate::crash_handler::install();
    crate::init_crypto();
    crate::resources::init();

    // TODO: once log-panics is released, can this be replaced by
    // log_panics::init()?
    panic::set_hook(Box::new(panic_hook::panic_hook));

    // Skip the first argument, which is the binary name.
    let args: Vec<String> = env::args().skip(1).collect();
    let (opts, mut preferences, mut servoshell_preferences) = match parse_command_line_arguments(&*args) {
        ArgumentParsingResult::ContentProcess(token) => return servo::run_content_process(token),
        ArgumentParsingResult::ChromeProcess(opts, preferences, servoshell_preferences) => {
            (opts, preferences, servoshell_preferences)
        },
        ArgumentParsingResult::Exit => {
            std::process::exit(0);
        },
        ArgumentParsingResult::ErrorParsing => {
            std::process::exit(1);
        },
    };

    crate::init_tracing(servoshell_preferences.tracing_filter.as_deref());

    #[cfg(feature = "headless-shell")]
    {
        if !servoshell_preferences.headless {
            info!(
                "headless-shell feature enabled; forcing headless startup and skipping headed rendering paths"
            );
        }
        servoshell_preferences.headless = true;

        if preferences.media_glvideo_enabled {
            warn!("GL video rendering is not supported on headless windows.");
            preferences.media_glvideo_enabled = false;
        }
    }

    if force_headless_requested() && !servoshell_preferences.headless {
        info!(
            "{FORCE_HEADLESS_ENV} is set; forcing headless startup and skipping Winit window initialization"
        );
        servoshell_preferences.headless = true;

        if preferences.media_glvideo_enabled {
            warn!("GL video rendering is not supported on headless windows.");
            preferences.media_glvideo_enabled = false;
        }
    }

    let clean_shutdown = servoshell_preferences.clean_shutdown;
    let event_loop = match servoshell_preferences.headless {
        true => ServoShellEventLoop::headless(),
        false => ServoShellEventLoop::headed(),
    };

    {
        let mut app = App::new(opts, preferences, servoshell_preferences, &event_loop);
        event_loop.run_app(&mut app);
    }

    crate::platform::deinit(clean_shutdown)
}
