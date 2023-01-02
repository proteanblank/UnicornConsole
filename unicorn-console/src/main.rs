mod gui;
mod input;
mod fps;
mod frametimes;
mod network;

use network::UnicornConsoleState;
use unicorn;

use crate::input::{LocalInputManager, MouseEventCollector, LocalPlayerId};
use unicorn::contexts::input_context::InputApi;

use crate::{
    gui::{framework::Framework, Gui},
};

use ggrs::{GGRSRequest, GGRSError, P2PSession, SessionState, Config};

use log::{debug, error, log_enabled, info, Level};
use env_logger;

use std::{
    path::PathBuf,
    time::{Duration, Instant},
    env,
    sync::{Arc, Mutex}
};

use pixels::{Pixels, SurfaceTexture};
use winit::{
    dpi::LogicalSize,
    event::{DeviceEvent, Event, MouseScrollDelta, VirtualKeyCode},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};
use winit_input_helper::WinitInputHelper;
use gilrs::Gilrs;

pub trait Console: Sized + Config {
    fn setup(&mut self);
    fn update(&mut self);
    fn draw(&mut self);
    fn blit(&self, buffer: &mut [u8]);
    fn frames_per_second(&mut self) -> usize;
    fn handle_requests(&mut self, requests: Vec<GGRSRequest<Self>>);
}

pub struct UnicornConsole {
    pub engine: Arc<Mutex<unicorn::core::Unicorn>>,
}

impl UnicornConsole {
    pub fn new(engine: unicorn::core::Unicorn) -> (Self, UnicornConsoleState) {
        let engine = Arc::new(Mutex::new(engine));

        let mut out = Self {
            engine,
        };

        let initial_state = out.generate_save_state();
        (out, initial_state)
    }

    pub fn width(&mut self) -> u32 {
        self.engine.lock().unwrap().width()
    }

    pub fn height(&mut self) -> u32 {
        self.engine.lock().unwrap().height()
    }
    
    fn generate_save_state(&mut self) -> UnicornConsoleState {
        UnicornConsoleState {
        }
    }
}

impl Console for UnicornConsole {
    fn frames_per_second(&mut self) -> usize {
        let engine = self.engine.lock().unwrap();
        
        engine.frame_rate.frames_per_second()
    }

    fn setup(&mut self) {
        self.engine.lock().unwrap().setup();
    }

    fn update(&mut self) {
        self.engine.lock().unwrap().update();
    }

    fn draw(&mut self) {
        self.engine.lock().unwrap().draw();
    }

    fn blit(&self, buffer: &mut [u8]) {
        let engine = self.engine.lock().unwrap();
        let screen = &mut engine.screen.lock().unwrap();
        buffer.copy_from_slice(&screen.pixel_buffer);
    }

    fn handle_requests(&mut self, requests: Vec<GGRSRequest<Self>>) {

        for request in requests {

            match request {
                GGRSRequest::SaveGameState { cell, frame } => {
                    let state = self.generate_save_state();
                    cell.save(frame, Some(state), None);
                }
                GGRSRequest::LoadGameState { cell, .. } => {
                    let state = cell.load().expect("Failed to load game state");
                   // self.load_save_state(state);
                }
                GGRSRequest::AdvanceFrame { inputs } => {
                    let engine = self.engine.lock().unwrap();
                    let contexts = &mut engine.contexts.lock().unwrap();
        
                        contexts.input_context
                        .input_entries
                        .iter_mut()
                        .zip(inputs.iter())
                        .for_each(|(current, new)| {
                            current.current = new.0.input_state;
                            current.current_mouse = new.0.mouse_state;
                        });

                 //  println!("{:?}", contexts.input_context.input_entries[0].current.buttons.get_button_state(unicorn::input::ButtonCode::ButtonA));

                 //  println!("{:?}", contexts.input_context.button_a_pressed(0));
                }
            }
        }
        self.update();
    }

}


fn main() -> Result<(), Box<dyn std::error::Error>> {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let mut session: Option<P2PSession<UnicornConsole>> = None;

   // let mut uc = unicorn::core::Unicorn::new();

    let mut gilrs = Gilrs::new().unwrap();

    let event_loop = EventLoop::new();

    let window = init_window(&event_loop);
    let window_size = window.inner_size();
    let scale_factor = window.scale_factor() as f32;
    let mut pixels = init_pixels(&window);

    let mut input = WinitInputHelper::new();
    let mut input_manager = LocalInputManager::new();
    let mut last_update = Instant::now();
    
    let mut times = frametimes::FrameTimes::new(Duration::from_secs(1) / 60);
    let mut fps_counter = fps::FpsCounter::new();
    let mut previous_frame_time = Instant::now();

    times.reset();

    let mut accumulator = Duration::ZERO;

    let mut framework = Framework::new(
        window_size.width,
        window_size.height,
        scale_factor,
        &pixels,
        Gui::default(),
        &event_loop,
    );

    let mut mouse_events = MouseEventCollector::default();

    event_loop.run(move |event, _, control_flow| {
        times.update();
        fps_counter.update(times.get_last_time());
      //  uc.fps = fps_counter.get_fps();

        if session.is_some() {
            if let Event::DeviceEvent { event, .. } = &event {
                if let DeviceEvent::MouseMotion { delta } = event {
                    mouse_events.delta_x += delta.0 as i16;
                    mouse_events.delta_y += delta.1 as i16;
                }

                if let DeviceEvent::MouseWheel { delta } = event {
                    let mut out_x = 0.0;
                    let mut out_y = 0.0;

                    match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            out_x += x;
                            out_y += y;
                        }
                        MouseScrollDelta::PixelDelta(d) => {
                            out_x += d.x as f32;
                            out_y += d.y as f32
                        }
                    }

                    if out_y > 0.0 {
                        mouse_events.wheel_down = true
                    } else if out_y < 0.0 {
                        mouse_events.wheel_up = true
                    }

                    if out_x > 0.0 {
                        mouse_events.wheel_right = true
                    } else if out_x < 0.0 {
                        mouse_events.wheel_left = true
                    }
                }
            }
        }

        if let Event::WindowEvent { event, .. } = &event {
            framework.handle_event(event);
        }

        framework.prepare(
            &mut pixels,
            &window,
            &mut session,
            &mut input_manager,
            &mut gilrs,
        );

        // Handle input events
        if input.update(&event) {
            // Close events
            if input.key_pressed(VirtualKeyCode::Escape) || input.quit() {
                *control_flow = ControlFlow::Exit;
                return;
            }

            // Put in pause
            if input.key_pressed(VirtualKeyCode::F1) {
                framework.gui.window_open = !framework.gui.window_open;
                //uc.switch_pause();
            }

            // Update the scale factor
            if let Some(scale_factor) = input.scale_factor() {
                framework.scale_factor(scale_factor);
            }

            // Resize the window
            if let Some(size) = input.window_resized() {
                pixels.resize_surface(size.width, size.height);
                framework.resize(size.width, size.height);
            }
        
            
           /* if uc.state == unicorn::core::UnicornState::RUN {

                uc.update();
                uc.draw();
            
                let now = Instant::now();
                let dt = now.duration_since(previous_frame_time);
                previous_frame_time = now;
                uc.update_time(dt);
            }*/

            if let Some(console) = &mut framework.gui.unicorn_console {
               let session = session.as_mut().unwrap();
               session.poll_remote_clients();

               if session.current_state() == SessionState::Running {
                    let mut fps_delta = 1. / console.frames_per_second() as f64;
                    if session.frames_ahead() > 0 {
                        fps_delta *= 1.1;
                    }

                    // get delta time from last iteration and accumulate it
                    let delta = Instant::now().duration_since(last_update);
                    accumulator = accumulator.saturating_add(delta);
                    last_update = Instant::now();

                    while accumulator.as_secs_f64() > fps_delta {
                        accumulator =
                        accumulator.saturating_sub(Duration::from_secs_f64(fps_delta));

                        // Process all the gamepad events
                        while gilrs.next_event().is_some() {}

                        let shared_mouse = std::mem::take(&mut mouse_events);

                        // Generate all local inputs
                        let mut local_player_id = LocalPlayerId(0);
                        for handle in session.local_player_handles() {
                            session
                                .add_local_input(
                                    handle,
                                    input_manager.generate_input_state(
                                        local_player_id,
                                        &pixels,
                                        &shared_mouse,
                                        &input,
                                        &gilrs,
                                    ),
                                )
                                .unwrap();
                            local_player_id.0 += 1;
                        }

                        // Update internal state
                        match session.advance_frame() {
                            Ok(requests) => {
                                console.handle_requests(requests);
                            }
                            Err(GGRSError::PredictionThreshold) => (),
                            Err(e) => panic!("{}", e),
                        }
                    }

                    console.draw();
                    console.blit(pixels.get_frame_mut());
               }
            }
            


            let render_result = pixels.render_with(|encoder, render_target, context| {
                context.scaling_renderer.render(encoder, render_target);
                framework.render(encoder, render_target, context)?;
                Ok(())
            });

            if render_result.is_err() {
                println!("render_with failed");
                *control_flow = ControlFlow::Exit;
                return;
            }

            window.request_redraw();
            //times.limit();
        }

    });

}

const DEFAULT_WINDOW_RESOLUTION: unicorn::core::resolution::Resolution = unicorn::core::resolution::Resolution::High;

fn init_window(event_loop: &EventLoop<()>) -> Window {
    let size = LogicalSize::new(
        DEFAULT_WINDOW_RESOLUTION.width() as f64,
        DEFAULT_WINDOW_RESOLUTION.height() as f64,
    );
    WindowBuilder::new()
        .with_title("Unicorn Console")
        .with_inner_size(size)
        .with_min_inner_size(size)
        .build(event_loop)
        .unwrap()
}


fn init_pixels(window: &Window) -> Pixels {
    let window_size = window.inner_size();
    let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);

    Pixels::new(128, 128, surface_texture).unwrap()
}