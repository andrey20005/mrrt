pub mod system;
pub mod green_app;
pub mod cube_app;
pub mod instance_cube;
pub mod shaders;
pub mod polygon;
pub mod bvh;
pub mod obj_parser;
pub mod ray_app;

use winit::event_loop::{ControlFlow, EventLoop};
use crate::system::App;

fn main() {
    // wgpu uses `log` for all of our logging, so we initialize a logger with the `env_logger` crate.
    //
    // To change the log level, set the `RUST_LOG` environment variable. See the `env_logger`
    // documentation for more information.
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();

    // When the current loop iteration finishes, immediately begin a new
    // iteration regardless of whether or not new events are available to
    // process. Preferred for applications that want to render as fast as
    // possible, like games.
    event_loop.set_control_flow(ControlFlow::Poll);

    // When the current loop iteration finishes, suspend the thread until
    // another event arrives. Helps keeping CPU utilization low if nothing
    // is happening, which is preferred if the application might be idling in
    // the background.
    // event_loop.set_control_flow(ControlFlow::Wait);

    // let mut app = crate::system::App::default();


    let mut app = App::<ray_app::RayApp>::new();
    event_loop.run_app(&mut app).unwrap();
}
