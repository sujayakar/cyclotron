mod db;
mod layout;
mod render;
mod view;

use crate::db::{Database};
use crate::layout::Layout;
use crate::view::View;
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
    let layout = Layout::new(&db);
    
    let event_loop = glutin::event_loop::EventLoop::new();
    let wb = glutin::window::WindowBuilder::new()
        .with_title(format!("Cyclotron: {}", args.trace));
    let cb = glutin::ContextBuilder::new()
        .with_depth_buffer(24)
        .with_multisampling(8);
    let display = glium::Display::new(wb, cb, &event_loop).unwrap();

    let mut view = View::new(&layout);
    let render = RenderState::new(&layout, &display);

    let mut last_name = None;
    let mut modifiers = glutin::event::ModifiersState::empty();
    let mut keys = NavKeys::default();

    event_loop.run(move |event, _, control_flow| {
        let next_frame_time = Instant::now() + Duration::from_nanos(16_666_667);
        *control_flow = glutin::event_loop::ControlFlow::WaitUntil(next_frame_time);

        match event {
            glutin::event::Event::WindowEvent { event, .. } => match event {
                glutin::event::WindowEvent::CloseRequested => {
                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                    return;
                },
                glutin::event::WindowEvent::CursorMoved { position, .. } => {
                    let dims = display.get_framebuffer_dimensions();
                    view.hover(&layout, (position.x / dims.0 as f64, position.y / dims.1 as f64));
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
                        _ => {}
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

        if modifiers == glutin::event::ModifiersState::empty() {
            let key_x_speed = match (keys.left, keys.right) {
                (true, false) => -10.0,
                (false, true) => 10.0,
                _ => 0.0,
            };
            let key_y_speed = match (keys.down, keys.up) {
                (true, false) => -5.0,
                (false, true) => 5.0,
                _ => 0.0,
            };
            view.scroll(&layout, key_x_speed, key_y_speed);
        }

        if let Some(selected) = view.selected_name() {
            if last_name != Some(selected) {
                println!("{:?}", db.name(selected));
                last_name = Some(selected);
            }
        }

        // frame_count += 1;

        let mut target = display.draw();
        target.clear_color_and_depth((1.0, 1.0, 1.0, 1.0), 1.0);

        render.draw(&view, &mut target);

        target.finish().unwrap();
    });
}
