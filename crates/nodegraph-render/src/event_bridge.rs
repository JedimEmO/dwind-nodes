use std::rc::Rc;

use dominator::events;
use nodegraph_core::interaction::{InputEvent, Modifiers, MouseButton};
use nodegraph_core::layout::Vec2;

use crate::graph_signals::GraphSignals;

fn convert_button(button: events::MouseButton) -> MouseButton {
    match button {
        events::MouseButton::Left => MouseButton::Left,
        events::MouseButton::Middle => MouseButton::Middle,
        events::MouseButton::Right => MouseButton::Right,
        _ => MouseButton::Left,
    }
}

pub fn on_mouse_down(gs: &Rc<GraphSignals>, e: events::MouseDown, container_rect: (f64, f64)) {
    let screen = Vec2::new(
        e.mouse_x() as f64 - container_rect.0,
        e.mouse_y() as f64 - container_rect.1,
    );
    let (pan_x, pan_y) = gs.pan.get();
    let zoom = gs.zoom.get();
    let world = Vec2::new((screen.x - pan_x) / zoom, (screen.y - pan_y) / zoom);
    gs.handle_input(InputEvent::MouseDown {
        screen,
        world,
        button: convert_button(e.button()),
        modifiers: Modifiers {
            shift: e.shift_key(),
            ctrl: e.ctrl_key(),
            alt: false,
        },
    });
}

pub fn on_mouse_move(gs: &Rc<GraphSignals>, e: events::MouseMove, container_rect: (f64, f64)) {
    let screen = Vec2::new(
        e.mouse_x() as f64 - container_rect.0,
        e.mouse_y() as f64 - container_rect.1,
    );
    let (pan_x, pan_y) = gs.pan.get();
    let zoom = gs.zoom.get();
    let world = Vec2::new((screen.x - pan_x) / zoom, (screen.y - pan_y) / zoom);
    gs.cursor_world.set((world.x, world.y));
    gs.handle_input(InputEvent::MouseMove {
        screen,
        world,
        modifiers: Modifiers {
            shift: e.shift_key(),
            ctrl: e.ctrl_key(),
            alt: false,
        },
    });
}

pub fn on_mouse_up(gs: &Rc<GraphSignals>, e: events::MouseUp, container_rect: (f64, f64)) {
    let screen = Vec2::new(
        e.mouse_x() as f64 - container_rect.0,
        e.mouse_y() as f64 - container_rect.1,
    );
    let (pan_x, pan_y) = gs.pan.get();
    let zoom = gs.zoom.get();
    let world = Vec2::new((screen.x - pan_x) / zoom, (screen.y - pan_y) / zoom);
    gs.handle_input(InputEvent::MouseUp {
        screen,
        world,
        button: convert_button(e.button()),
        modifiers: Modifiers {
            shift: e.shift_key(),
            ctrl: e.ctrl_key(),
            alt: false,
        },
    });
}

pub fn on_wheel(gs: &Rc<GraphSignals>, e: events::Wheel, container_rect: (f64, f64)) {
    let screen = Vec2::new(
        e.mouse_x() as f64 - container_rect.0,
        e.mouse_y() as f64 - container_rect.1,
    );
    let delta = -e.delta_y();
    gs.handle_input(InputEvent::Scroll { screen, delta });
}
