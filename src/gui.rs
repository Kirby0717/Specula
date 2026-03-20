mod atlas;
mod renderer;
mod window;

pub use atlas::GlyphAtlas;
pub use renderer::{GpuContext, Renderer};

pub fn run_app() -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    event_loop.run_app(&mut window::AppHandler::default())?;
    Ok(())
}
