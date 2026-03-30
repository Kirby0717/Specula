mod app;
mod atlas;
mod renderer;
mod window;

use app::App;
pub use atlas::GlyphAtlas;
pub use renderer::{GpuContext, Renderer};

pub(super) enum TermEvent {
    PtyOutput,
    PtyExit,
}

pub fn run_app(config: crate::config::Config) -> anyhow::Result<()> {
    let event_loop =
        winit::event_loop::EventLoop::<TermEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
    event_loop.run_app(&mut window::AppHandler::new(proxy, config))?;
    Ok(())
}
