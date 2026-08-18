use std::time::Instant;

pub struct FrameTimeCounter {
    // Кольцевой буфер длительностей кадров в секундах
    buffer: Vec<f32>,
    // Индекс для записи следующего кадра
    head: usize,
    // Количество фактически записанных кадров в буфер (до переполнения len == head)
    recorded_count: usize,
    
    // Общее целевое время хранения истории в секундах (задается в нью)
    history_duration: f32,
    // Таймер для автоматического подсчета дельты кадра
    last_tick: Instant,

    // Постоянный временный буфер для расчета процентилей (сохраняет capacity)
    sort_buffer: Vec<f32>,
}

impl FrameTimeCounter {
    pub fn new(history_seconds: f32) -> Self {
        // Стартовый размер буфера (например, с запасом на 1000 кадров)
        let initial_capacity = 1000;
        Self {
            buffer: vec![0.0; initial_capacity],
            head: 0,
            recorded_count: 0,
            history_duration: history_seconds,
            last_tick: Instant::now(),
            sort_buffer: Vec::with_capacity(initial_capacity),
        }
    }

    /// Фиксирует завершение кадра, автоматически вычисляет дельту времени и пушит в буфер
    pub fn tick(&mut self) {
        let now = Instant::now();
        let delta_seconds = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;

        // Записываем дельту кадра в текущую голову кольца
        self.buffer[self.head] = delta_seconds;
        
        // Сдвигаем голову вперед
        self.head += 1;
        if self.recorded_count < self.buffer.len() {
            self.recorded_count = self.head;
        }

        // Если дошли до физического конца вектора, проверяем, хватает ли нам истории.
        // Если общая сумма секунд в кольце меньше требуемой, или мы просто зациклились,
        // но нам нужно гарантировать автоматическое расширение по вашему условию:
        if self.head >= self.buffer.len() {
            let total_stored_time: f32 = self.buffer.iter().sum();
            
            if total_stored_time < self.history_duration {
                // Буфера не хватило, чтобы вместить нужную историю времени -> удваиваем
                let old_size = self.buffer.len();
                let new_size = old_size * 2;
                
                // Создаем новый увеличенный буфер
                let mut new_buffer = vec![0.0; new_size];
                // Копируем старые данные, распрямляя кольцо (от 0 до old_size)
                new_buffer[..old_size].copy_from_slice(&self.buffer);
                
                self.buffer = new_buffer;
                self.head = old_size; // Новая голова указывает на продолжение массива
                self.recorded_count = self.head;
            } else {
                // Если истории по времени хватает, просто зацикливаем кольцо
                self.head = 0;
            }
        }
    }

    /// Считает средний FPS за последние m_seconds. 
    /// Если истории меньше, чем m_seconds, считает среднее по тому, что зафиксировано.
    pub fn get_avg_fps(&self, m_seconds: f32) -> f32 {
        if self.recorded_count == 0 {
            return 0.0;
        }

        let mut sum_time = 0.0;
        let mut frame_count = 0;
        
        // Двигаемся от головы буфера назад во времени
        let mut curr = if self.head == 0 { self.buffer.len() - 1 } else { self.head - 1 };
        let steps = self.recorded_count;

        for _ in 0..steps {
            let t = self.buffer[curr];
            sum_time += t;
            frame_count += 1;

            if sum_time >= m_seconds {
                break;
            }

            curr = if curr == 0 { self.buffer.len() - 1 } else { curr - 1 };
        }

        if sum_time > 0.0 {
            frame_count as f32 / sum_time
        } else {
            0.0
        }
    }

    /// Считает процентиль частоты кадров (например, 0.01 для 1% Low) за последние m_seconds.
    /// Если истории меньше, чем m_seconds, рассчитывает процентиль по всей доступной истории.
    pub fn get_percentile_fps(&mut self, percentile: f32, m_seconds: f32) -> f32 {
        if self.recorded_count == 0 {
            return 0.0;
        }

        // Очищаем временный буфер сортировки, сохраняя его capacity!
        self.sort_buffer.clear();

        let mut sum_time = 0.0;
        let mut curr = if self.head == 0 { self.buffer.len() - 1 } else { self.head - 1 };
        let steps = self.recorded_count;

        // Собираем кадры, попавшие в окно времени
        for _ in 0..steps {
            let t = self.buffer[curr];
            sum_time += t;
            self.sort_buffer.push(t);

            if sum_time >= m_seconds {
                break;
            }

            curr = if curr == 0 { self.buffer.len() - 1 } else { curr - 1 };
        }

        if self.sort_buffer.is_empty() {
            return 0.0;
        }

        // Сортируем длительности кадров по возрастанию (от быстрых кадров к медленным фризам)
        self.sort_buffer.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Вычисляем индекс искомого процентиля. 
        // Для 1% Low (худшие кадры) нам нужны самые ДЛИННЫЕ кадры по времени, 
        // поэтому индекс берется с конца отсортированного массива: (1.0 - percentile)
        let target_idx = ((self.sort_buffer.len() as f32) * (1.0 - percentile)) as usize;
        let target_idx = target_idx.min(self.sort_buffer.len() - 1);

        let longest_frame_time = self.sort_buffer[target_idx];
        
        if longest_frame_time > 0.0 {
            1.0 / longest_frame_time
        } else {
            0.0
        }
    }
}
