use glam::{Mat3, Vec3};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

pub struct Camera {
    // Позиция камеры в мировых координатах
    position: Vec3,
    
    // Углы поворота в радианах (Pitch - широта/вертикаль, Yaw - долгота/горизонталь)
    pitch: f32,
    yaw: f32,
    
    // Внутреннее число для экспоненциального зума (от -inf до inf)
    zoom_exponent: f32,

    // Чувствительность управления
    move_speed: f32,
    mouse_sensitivity: f32,
    scroll_sensitivity: f32,

    // Состояния кнопок для плавного перемещения в render()
    key_w: bool,
    key_a: bool,
    key_s: bool,
    key_d: bool,
    key_space: bool,
    key_shift: bool,

    // Флаг зажатия Левой Кнопки Мыши
    is_lkm_pressed: bool,
    pub is_space_mode: bool,
}

impl Camera {
    pub fn new(position: Vec3, pitch_deg: f32, yaw_deg: f32, move_speed: f32, is_space_mode: bool) -> Self {
        Self {
            position,
            pitch: pitch_deg.to_radians(),
            yaw: yaw_deg.to_radians(),
            zoom_exponent: 0.0, // e^0 = 1.0 (дефолтный зум)
            move_speed,
            mouse_sensitivity: 0.005, // в радианах на логический пиксель
            scroll_sensitivity: 0.1,
            key_w: false,
            key_a: false,
            key_s: false,
            key_d: false,
            key_space: false,
            key_shift: false,
            is_lkm_pressed: false,
            is_space_mode,
        }
    }

    // --- ГЕТТЕРЫ ДЛЯ UNIFORM БУФЕРА ---

    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// Возвращает экспоненциальный зум: всегда от 0.0 до +inf
    pub fn zoom(&self) -> f32 {
        self.zoom_exponent.exp()
    }

    /// Рассчитывает матрицу вращения 3х3 на основе текущих углов
    pub fn rotation_matrix(&self) -> Mat3 {
        // Создаем вращение вокруг осей X и Y
        let rot_x = Mat3::from_rotation_x(-self.pitch);
        let rot_y = Mat3::from_rotation_y(self.yaw);
        // Матрица трансформации направлений
        rot_y * rot_x
    }

    // --- ОБРАБОТКА ВВОДА ---

    pub fn handle_input(&mut self, event: &WindowEvent) -> bool {
        match event {
            // Отслеживаем нажатие ЛКМ
            WindowEvent::MouseInput { button: MouseButton::Left, state, .. } => {
                self.is_lkm_pressed = *state == ElementState::Pressed;
                true
            }

            // Отслеживаем движение мыши для поворота камеры
            WindowEvent::CursorMoved { .. } => {
                false
            }

            // Отслеживаем прокрутку колесика для ЗУМА
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_delta = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.02,
                };
                // Изменяем экспоненту линейно (может уходить в минус или плюс)
                self.zoom_exponent += scroll_delta * self.scroll_sensitivity;
                true
            }

            // 4. Отслеживаем клавиатуру (WASD / Space / Shift)
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(key_code), state, .. }, .. } => {
                let is_pressed = *state == ElementState::Pressed;
                match key_code {
                    KeyCode::KeyW | KeyCode::ArrowUp => self.key_w = is_pressed,
                    KeyCode::KeyS | KeyCode::ArrowDown => self.key_s = is_pressed,
                    KeyCode::KeyA | KeyCode::ArrowLeft => self.key_a = is_pressed,
                    KeyCode::KeyD | KeyCode::ArrowRight => self.key_d = is_pressed,
                    KeyCode::Space => self.key_space = is_pressed,
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => self.key_shift = is_pressed,
                    _ => return false, // Кнопка не относится к камере
                }
                true
            }
            _ => false,
        }
    }

    /// Метод для плавного перемещения мыши (вызывается из system.rs при DeviceEvent::MouseMotion)
    pub fn handle_mouse_motion(&mut self, dx: f64, dy: f64) {
        if self.is_lkm_pressed {
            // Сдвиг мыши по горизонтали (dx) меняет долготу (Yaw)
            self.yaw += (dx as f32) * self.mouse_sensitivity;
            
            // Сдвиг мыши по вертикали (dy) меняет широту (Pitch)
            self.pitch -= (dy as f32) * self.mouse_sensitivity;

            // Жестко ограничиваем вертикальный наклон от -90 до 90 градусов в радианах
            let limit = 89.0f32.to_radians();
            self.pitch = self.pitch.clamp(-limit, limit);

            // Зацикливаем Yaw от 0 до 2*PI, чтобы число не росло бесконечно
            let two_pi = 2.0 * std::f32::consts::PI;
            self.yaw = self.yaw % two_pi;
            if self.yaw < 0.0 {
                self.yaw += two_pi;
            }
        }
    }

    /// Обновляет позицию камеры на основе зажатых кнопок. 
    /// Должен вызываться каждый кадр в самом начале метода RayApp::render()!
    pub fn update_position(&mut self, delta_time: f32) {
        let mut move_vector = Vec3::ZERO;
        let rot_mat = self.rotation_matrix();

        if self.is_space_mode {
            // ==================================================
            // РЕЖИМ КОСМОСА: Все направления строго относительно взгляда
            // ==================================================
            let forward = rot_mat * Vec3::new(0.0, 0.0, 1.0);
            let right   = rot_mat * Vec3::new(1.0, 0.0, 0.0);
            let up      = rot_mat * Vec3::new(0.0, 1.0, 0.0);

            if self.key_w     { move_vector += forward; }
            if self.key_s     { move_vector -= forward; }
            if self.key_d     { move_vector += right; }
            if self.key_a     { move_vector -= right; }
            if self.key_space { move_vector += up; }
            if self.key_shift { move_vector -= up; }
        } else {
            // ==================================================
            // РЕЖИМ ПОЛЁТА: WASD на плоскости XZ, Space/Shift строго по глобальной оси Y
            // ==================================================
            // Считаем классический forward на плоскости земли, зная только угол Yaw (поворот по горизонтали)
            // В правой системе координат: sin(yaw) дает смещение по X, а -cos(yaw) дает смещение по Z
            let forward = Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos());
            
            // Вектор право всегда перпендикулярен вектору вперед на плоскости земли
            let right = Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin());
            
            // Верх всегда строго глобальный
            let up = Vec3::Y;

            if self.key_w     { move_vector += forward; }
            if self.key_s     { move_vector -= forward; }
            if self.key_d     { move_vector += right; }
            if self.key_a     { move_vector -= right; }
            if self.key_space { move_vector += up; }
            if self.key_shift { move_vector -= up; }
        }

        // Применяем итоговое смещение
        if move_vector != Vec3::ZERO {
            self.position += move_vector.normalize() * self.move_speed * delta_time;
        }
    }
}

