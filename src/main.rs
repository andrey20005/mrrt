use winit::event_loop::{ControlFlow, EventLoop};

// pub mod system;
pub mod green_app;
// pub mod cube_app;
pub mod shaders;
pub mod polygon;
pub mod bvh;
pub mod obj_parser;
pub mod ray_app;
pub mod fps_counter;
pub mod camera;
pub mod app_prelude;
pub mod test_app;
pub mod texture_mapping;

fn main() {
    // wgpu использует `log` для всего нашего логирования,
    // поэтому мы инициализируем логгер с помощью крейта `env_logger`.
    //
    // Чтобы изменить уровень логирования, установите переменную
    // окружения `RUST_LOG`. См. документацию `env_logger`
    // для получения дополнительной информации.
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();

    // Когда текущая итерация цикла завершается, немедленно
    // начинается новая итерация, независимо от того, доступны ли
    // новые события для обработки. Рекомендуется для приложений,
    // которые хотят рендерить как можно быстрее, например для игр.
    event_loop.set_control_flow(ControlFlow::Poll);

    // Когда текущая итерация цикла завершается, поток
    // приостанавливается до тех пор, пока не поступит другое событие.
    // Помогает сохранять низкое использование CPU, если ничего не
    // происходит, что предпочтительно, если приложение может
    // простаивать в фоновом режиме.
    // event_loop.set_control_flow(ControlFlow::Wait);

    // с помощью дженерика указываем логику приложения
    // let mut app = app_prelude::App::<green_app::GreenApp>::new();
    let mut app = app_prelude::App::<test_app::TestApp>::new();
    // let mut app = app_prelude::App::<ray_app::RayApp>::new();
    event_loop.run_app(&mut app).unwrap();
}
