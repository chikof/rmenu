use std::process::{Command, Stdio};

use completions::path::get_path_programs;
use components::match_selector::pager::Pager;
use components::text_input::TextInput;
use config::loader::Config;
use config::types::WindowPosition;
use flexi_logger::{Logger, colored_default_format};
use log::{error, info, warn};
use sdl2::event::Event;
use sdl2::gfx::primitives::DrawRenderer;
use sdl2::init as sdl2_init;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::rwops::RWops;
use sdl2::version::version as sdl2_version;
use utils::errors::handle_app_error;
use utils::misc::{find_mouse_monitor, ttf_context};
use utils::vector_matrix::{Vector2I, Vector2U};

mod completions;
mod components;
mod config;
mod utils;

fn main() {
    Logger::try_with_str("DEBUG")
        .expect("To start logger with DEBUG.")
        .format(colored_default_format)
        .start()
        .expect("To start logger.");

    info!("Staring r-menu version {}", env!("CARGO_PKG_VERSION"));

    let sdl_context = handle_app_error!(sdl2_init());
    let ttf_context = handle_app_error!(ttf_context());

    info!("Initialized SDL2 {}", sdl2_version());

    // TODO: find the screen id instead of the window id.
    let video_subsystem = handle_app_error!(sdl_context.video());
    let display_bounds = handle_app_error!(video_subsystem.display_bounds({
        let monitor_id =
            handle_app_error!(find_mouse_monitor(&video_subsystem)).unwrap_or_else(|| {
                warn!("Couldn't get which monitor the mouse is in, falling back to 0");
                0
            });

        info!("Detected monitor id {monitor_id}");

        monitor_id
    }));

    let default_font = handle_app_error!(ttf_context.load_font_from_rwops(
        handle_app_error!(RWops::from_bytes(include_bytes!("../assets/default_font.ttf"))),
        14
    ));

    macro_rules! window {
        ([$x:expr, $y:expr], [$width:expr, $height:expr]) => {
            handle_app_error!(
                video_subsystem
                    .window("r-menu", $width, $height)
                    .position($x, $y)
                    .borderless()
                    .always_on_top()
                    .build()
            )
            .into_canvas()
            .present_vsync()
            .build()
            .map_err(|e| e.to_string())
        };
    }

    let config = match Config::load() {
        Ok(config) => config,
        Err(err) => {
            // This branch opens a window with default configuration,
            // and this window is used to display a configuration error,
            // so the user doesn't need to checkout logs every time.

            // The window has similar properties to dmenu.

            let mut canvas = handle_app_error!(window!(
                [display_bounds.x(), display_bounds.y()],
                [display_bounds.width(), 20]
            ));

            warn!("Error detected. Error window opened");
            error!("{err:#}");

            let texture_creator = canvas.texture_creator();
            let error_text_surface = handle_app_error!(
                default_font
                    .render(&format!("{err:#} | Press <ESC> or <RETURN> to exit."))
                    .blended(Color::RED)
            );
            let error_text_texture =
                handle_app_error!(texture_creator.create_texture_from_surface(&error_text_surface));

            let mut event_pump = handle_app_error!(sdl_context.event_pump());
            'event_loop: loop {
                for event in event_pump.poll_iter() {
                    if let Event::KeyDown {
                        keycode: Some(Keycode::Escape | Keycode::Return),
                        ..
                    } = event
                    {
                        break 'event_loop;
                    }
                }

                canvas.set_draw_color(Color::RGB(20, 20, 20));
                canvas.clear();

                handle_app_error!(canvas.filled_circle(10, 10, 5, Color::RED));

                handle_app_error!(canvas.copy(
                    &error_text_texture,
                    None,
                    Some(Rect::new(25, 0, error_text_surface.width(), error_text_surface.height()))
                ));

                canvas.present();
            }

            info!("See ya!");
            return;
        },
    };

    let font = config
        .font()
        .unwrap_or(&default_font);

    let window_rect = {
        let config_padding = config.window_padding();
        let config_height = config.window_height();

        let window_position = Vector2I::new(
            display_bounds.x() + (config_padding.x() / 2.0) as i32,
            match config.window_position() {
                WindowPosition::Top => display_bounds.y() + (config_padding.y() / 2.0) as i32,
                WindowPosition::Bottom => {
                    display_bounds.y() + display_bounds.height() as i32
                        - config_height as i32
                        - (config_padding.y() / 2.0) as i32
                },
            },
        );

        let window_size =
            Vector2U::new(display_bounds.width() - config_padding.x() as u32, config_height as u32);

        Rect::new(window_position.x(), window_position.y(), window_size.x(), window_size.y())
    };

    let mut canvas = handle_app_error!(window!(
        [window_rect.x(), window_rect.y()],
        [window_rect.width(), window_rect.height()]
    ));

    info!("Started window, requested: {window_rect:?}");

    let texture_creator = canvas.texture_creator();

    let mut input = TextInput::new(&font);
    input.set_color(config.text_color());
    input.set_position(Vector2I::new(0, 0));

    let minus_a_quarter_window = (window_rect.width() / 2) / 2;

    let mut pager = Pager::new(
        handle_app_error!(get_path_programs())
            .into_iter()
            .collect(),
        &font,
    );
    pager.set_position(Vector2I::new(minus_a_quarter_window as i32, 0));
    pager.set_size(Vector2U::new(
        window_rect.width() - minus_a_quarter_window,
        window_rect.height(),
    ));
    pager.set_text_color(config.text_color());
    pager.set_highlight_color(config.highlight_color());
    pager.set_highlighted_text_color(config.highlighted_text_color());
    handle_app_error!(pager.compute_text(""));

    let mut shift_pressed = false;
    let mut in_args = false;

    let mut event_pump = handle_app_error!(sdl_context.event_pump());
    'event_loop: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'event_loop,

                Event::KeyDown { keycode: Some(keycode), .. } => match keycode {
                    Keycode::Escape => {
                        info!("Cheerio.");
                        break 'event_loop;
                    },

                    Keycode::LShift | Keycode::RShift => {
                        shift_pressed = true;
                    },

                    Keycode::Tab => {
                        if !in_args {
                            if let Some(selected) = pager.get_selected_entry() {
                                input.set_text(
                                    selected
                                        .item()
                                        .get_text(),
                                );
                            }
                        }
                    },

                    Keycode::Return => {
                        let input_args = input.get_args();

                        let mut command = match pager.get_selected_entry() {
                            Some(selected) if input_args.len() <= 1 => {
                                info!(
                                    "Requesting to start '{}'",
                                    selected
                                        .item()
                                        .get_text()
                                );
                                Command::new(
                                    selected
                                        .item()
                                        .get_text(),
                                )
                            },

                            _ => {
                                if input_args.is_empty() {
                                    continue;
                                }

                                let mut command = Command::new(&input_args[0]);

                                if input_args.len() > 1 {
                                    command.args(&input_args[1..]);
                                }

                                info!("Requesting to start '{}'", input_args.join(" "));

                                command
                            },
                        };

                        command.stdout(Stdio::null());
                        command.stderr(Stdio::null());
                        command.stdin(Stdio::null());

                        #[cfg(unix)]
                        unsafe {
                            use std::os::unix::process::CommandExt;

                            command.pre_exec(|| {
                                sdl2::libc::setsid();
                                Ok(())
                            });
                        }

                        #[cfg(windows)]
                        {
                            use std::os::windows::process::CommandExt;

                            command.creation_flags(0x00000008);
                        }

                        handle_app_error!(command.spawn());

                        info!("Started gracefully... Have a jolly good day!");

                        break 'event_loop;
                    },

                    keycode => {
                        if input.is_caret_at_end() {
                            pager.keycode_interaction(keycode);
                        }

                        if pager.is_caret_at_start() {
                            input.keycode_interaction(keycode);
                        }

                        input.act_char_at_caret(keycode, shift_pressed);

                        let input_args = input.get_args();

                        if let Some(program_name) = input_args.get(0) {
                            handle_app_error!(pager.compute_text(program_name));
                        }

                        in_args = input_args.len() > 1;
                    },
                },

                Event::KeyUp { keycode: Some(keycode), .. } => match keycode {
                    Keycode::LShift | Keycode::RShift => {
                        shift_pressed = false;
                    },

                    _ => {},
                },

                _ => {},
            }
        }

        canvas.set_draw_color(config.window_background_color());
        canvas.clear();

        handle_app_error!(input.draw(&mut canvas, &texture_creator));

        if !in_args {
            handle_app_error!(pager.draw(&mut canvas));
        }

        canvas.present();
    }
}
