mod db;
mod layout;
mod render;
mod view;
mod util;

use std::io::Write;
use crate::db::{Database};
use crate::layout::Layout;
use crate::view::{View, SelectionInfo};
use crate::render::RenderState;

use glium::{
    glutin,
    Surface,
};
use structopt::StructOpt;
use std::time::{Duration, Instant};

#[derive(Debug, StructOpt)]
struct Args {
    trace: String,
    #[structopt(long)]
    show_framerate: bool,
    #[structopt(default_value="60")]
    target_framerate: f64,
    #[structopt(long)]
    no_wakes_printing: bool,
    // grep: Vec<String>,
    // hide_wakeups: Vec<String>,
}

#[derive(Default)]
struct NavKeys {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

fn main() {
    let args = Args::from_args();

    let db = Database::load(&args.trace);
    let mut layout = Layout::new(&db, None);
    
    let event_loop = glutin::event_loop::EventLoop::new();
    let wb = glutin::window::WindowBuilder::new()
        .with_title(format!("Cyclotron: {}", args.trace));
    let cb = glutin::ContextBuilder::new()
        .with_depth_buffer(24)
        .with_multisampling(8);
    let display = glium::Display::new(wb, cb, &event_loop).unwrap();

    let mut view = View::new(&layout);
    let mut render = RenderState::new(&layout, &display);

    let target_frame_delta = Duration::from_nanos((1e9 / args.target_framerate) as u64);

    let mut click_down_time = None;
    let mut last_name = None;
    let mut modifiers = glutin::event::ModifiersState::empty();
    let mut keys = NavKeys::default();
    let mut span_stack = Vec::new();

    let mut last_frame = Instant::now();
    let mut frame_rates = Vec::new();

    enum InputMode {
        Navigate,
        Search(String),
    }
    let mut input_mode = InputMode::Navigate;

    event_loop.run(move |event, _, control_flow| {
        let now = Instant::now();

        match event {
            glutin::event::Event::WindowEvent { event, .. } => match event {
                glutin::event::WindowEvent::CloseRequested => {
                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                    return;
                },
                glutin::event::WindowEvent::CursorMoved { position, .. } => {
                    let dims = display.get_framebuffer_dimensions();
                    view.hover(&layout, (position.x / dims.0 as f64, position.y / dims.1 as f64));
                    // println!("cursor time: {:?}", Duration::from_nanos(view.cursor_time()));
                }
                glutin::event::WindowEvent::ReceivedCharacter(ch) => {
                    match &mut input_mode {
                        InputMode::Navigate => {
                            if ch == '/' {
                                input_mode = InputMode::Search(String::new());
                            }
                        }
                        InputMode::Search(ref mut text) => {
                            if ch == '\r' {
                                println!("Search {:?}", text);
                                let new_layout = Layout::new(&db, Some(&text));
                                let span_count = new_layout.span_count();
                                println!("  found {} spans", span_count);
                                if span_count > 0 {
                                    layout = new_layout;
                                    println!("  (type <slash><return> to get return to normal view)");

                                    view.relayout(&layout);

                                    if text == "" {
                                        view.set_span_full(&layout);
                                    }
                                    render = RenderState::new(&layout, &display);
                                }

                                input_mode = InputMode::Navigate;
                            } else {
                                text.push(ch);
                            }
                        }
                    }
                }
                glutin::event::WindowEvent::ModifiersChanged(new) => {
                    modifiers = new;
                }
                glutin::event::WindowEvent::KeyboardInput { input: glutin::event::KeyboardInput {
                    state, virtual_keycode: Some(key), ..
                }, .. } => {
                    let pressed = match state {
                        glutin::event::ElementState::Pressed => true,
                        glutin::event::ElementState::Released => false,
                    };

                    match &input_mode {
                        InputMode::Navigate => {
                            match key {
                                glutin::event::VirtualKeyCode::W | glutin::event::VirtualKeyCode::Up => {
                                    keys.up = pressed;
                                }
                                glutin::event::VirtualKeyCode::A | glutin::event::VirtualKeyCode::Left => {
                                    keys.left = pressed;
                                }
                                glutin::event::VirtualKeyCode::S | glutin::event::VirtualKeyCode::Down => {
                                    keys.down = pressed;
                                }
                                glutin::event::VirtualKeyCode::D | glutin::event::VirtualKeyCode::Right => {
                                    keys.right = pressed;
                                }
                                glutin::event::VirtualKeyCode::Q => {
                                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                                    return;
                                }
                                glutin::event::VirtualKeyCode::P => {
                                    if pressed {
                                        view.toggle_mode(&layout);
                                    }
                                }
                                glutin::event::VirtualKeyCode::Escape if pressed => {
                                    if let Some(span) = span_stack.pop() {
                                        view.set_span(&layout, span)
                                    } else {
                                        view.set_span_full(&layout);
                                    }
                                }
                                _ => {}
                            }
                        }
                        InputMode::Search(_) => {
                            match key {
                                glutin::event::VirtualKeyCode::Escape if pressed => {
                                    input_mode = InputMode::Navigate;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                glutin::event::WindowEvent::MouseInput { state, button: glutin::event::MouseButton::Left, .. } => {
                    match state {
                        glutin::event::ElementState::Pressed => {
                            click_down_time = Some(Instant::now());
                            view.begin_drag()
                        }
                        glutin::event::ElementState::Released => {
                            if click_down_time.unwrap().elapsed() > Duration::from_millis(100) {
                                span_stack.push(view.end_drag());
                            } else {
                                view.cancel_drag();
                            }
                        },
                    }
                }
                _ => {
                    // println!("{:?}", event);
                    return
                },
            },
            glutin::event::Event::NewEvents(cause) => match cause {
                glutin::event::StartCause::ResumeTimeReached { .. } => (),
                glutin::event::StartCause::Init => (),
                _ => return,
            },
            glutin::event::Event::MainEventsCleared | 
            glutin::event::Event::RedrawEventsCleared => return,
            glutin::event::Event::DeviceEvent { event, .. } => match event {
                glutin::event::DeviceEvent::MouseWheel { delta: 
                    glutin::event::MouseScrollDelta::PixelDelta(delta) } => {
                    view.scroll(&layout, -delta.x, delta.y);
                }
                _ => {}
            },
            _ => {
                return;
            }
        }
        
        let next_frame_time = now + target_frame_delta;
        let elapsed = now - last_frame;
        last_frame = now;
        *control_flow = glutin::event_loop::ControlFlow::WaitUntil(next_frame_time);

        if modifiers == glutin::event::ModifiersState::empty() {
            let elapsed = elapsed.as_secs_f64();
            let factor = 400.0;
            let key_x_speed = match (keys.left, keys.right) {
                (true, false) => -factor,
                (false, true) => factor,
                _ => 0.0,
            };
            let key_y_speed = match (keys.down, keys.up) {
                (true, false) => -factor,
                (false, true) => factor,
                _ => 0.0,
            };
            if key_x_speed != 0.0 || key_y_speed != 0.0 {
                view.scroll(&layout, 2.0 * key_x_speed * elapsed, key_y_speed * elapsed);
            }
        }

        if args.show_framerate {
            frame_rates.push(elapsed.as_nanos() as u64);

            if frame_rates.len() == 60 {
                print!("\r\x1b[0Kaverage {:.3} worst {:.3}",
                    1e9 * frame_rates.len() as f64 / frame_rates.iter().sum::<u64>() as f64,
                    1e9 / *frame_rates.iter().max().unwrap() as f64);
                std::io::stdout().flush().unwrap();
                frame_rates.clear();
            }

        } else if let Some(selected) = view.selection() {
            if last_name != Some(selected) {
                match selected {
                    SelectionInfo::Span { name, span, task } => {
                        println!("start {:?} length {:?} : {}",
                            Duration::from_nanos(span.begin),
                            Duration::from_nanos(span.end - span.begin),
                            db.name(name));

                        if !args.no_wakes_printing {
                            let parks = db.parks(task);
                            for wake in parks {
                                println!("    woken by: {}", db.name(db.task(wake.waking).name));
                            }

                            let wakes = db.wakes(task);
                            for wake in wakes {
                                println!("    wakes: {}", db.name(db.task(wake.parked).name));
                            }
                        }
                    }
                    SelectionInfo::ProfileName { name, time } => {
                        println!("time {:?} ({:.2}%) : {}",
                            Duration::from_nanos(time),
                            time as f32 / view.span_time() as f32 * 100.0,
                            db.name(name));
                    }
                }

                last_name = Some(selected);
            }
        }

        let mut target = display.draw();
        target.clear_color_and_depth((1.0, 1.0, 1.0, 1.0), 1.0);

        render.draw(&view, &mut target);

        target.finish().unwrap();
    });
}
