use heapless::{consts, spsc};
use nannou_audio::Buffer;
use rand::{thread_rng, Rng};

pub const SAMPLE_RATE: usize = 44_100;
pub const NUM_CHANNELS: usize = 2;
pub const BUFFER_SIZE: usize = 2048;
pub const NUM_GRAINS: usize = 8;
pub const NUM_VOICES: usize = 4;

pub type Consumer = spsc::Consumer<'static, Voices, consts::U16>;
pub type Producer = spsc::Producer<'static, Voices, consts::U16>;
pub type Queue = spsc::Queue<Voices, consts::U16>;

#[derive(Clone, Copy, Debug)]
pub struct Grain {
    pub active: bool,
    pub volume: f32,
    pub pan: f32,
    pub lut: rume::Lut<'static>,
    pub slice: &'static [f32],
    env_position: f32,
    env_increment: f32,
}

unsafe impl Send for Grain {}
unsafe impl Sync for Grain {}

fn random_slice(table: &[f32]) -> &[f32] {
    let mut rng = thread_rng();
    let table_len = table.len() as f32;
    let start = (rng.gen_range(0.0..0.4) * table_len) as usize;
    let length = (rng.gen_range(0.8..1.0) * table_len) as usize;
    let end = (start + length).min(table.len());
    &table[start..end]
}

impl Grain {
    fn new(table: &'static [f32], pitch: Option<f32>) -> Self {
        let mut grain = Grain::generate(table, pitch.unwrap_or(220.0));
        grain.active = false;
        grain
    }

    fn generate(table: &'static [f32], pitch: f32) -> Self {
        let mut rng = thread_rng();
        let slice = random_slice(table);
        let lut_increment = pitch * rume::convert::pitch::from_midi(60.0) / SAMPLE_RATE as f32;
        let env_increment = lut_increment / slice.len() as f32;

        Self {
            active: true,
            env_increment,
            slice,
            lut: {
                let mut lut = rume::Lut::new(&slice);
                lut.phasor.inc(lut_increment);
                lut
            },
            volume: rng.gen_range(0.0f32..1.0f32).powf(0.3),
            pan: rng.gen_range(0.0..1.0),
            env_position: 0.0,
        }
    }

    pub fn advance(&mut self) -> (f32, f32) {
        if !self.active {
            return (0.0, 0.0);
        }

        let vol = self.env() * self.volume;
        let sample = self.lut.step();
        self.pan(sample * vol)
    }

    fn pan(&self, sample: f32) -> (f32, f32) {
        (sample * (1.0 - self.pan), sample * self.pan)
    }

    fn env(&mut self) -> f32 {
        let env = if self.env_position > 0.5 {
            ((1.0 - self.env_position) * 8.0).min(1.0)
        } else {
            (self.env_position * 8.0).min(1.0)
        };

        self.env_position += self.env_increment;

        if self.env_position >= 1.0 {
            self.env_position = 0.0;
            self.active = false;
        }

        env
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Grains {
    pub grains: [Grain; NUM_GRAINS],
    table: &'static [f32],
}

impl Grains {
    fn new(table: &'static [f32]) -> Self {
        Self {
            grains: [Grain::new(table, None); NUM_GRAINS],
            table,
        }
    }

    fn activate(&mut self, pitch: f32) -> Result<(), ()> {
        for grain in self.grains.iter_mut() {
            if !grain.active {
                *grain = Grain::generate(self.table, pitch);
                return Ok(());
            }
        }
        Err(())
    }

    fn advance(&mut self) -> (f32, f32) {
        const INV_NUM_GRAINS: f32 = 1.0 / NUM_GRAINS as f32;
        self.grains
            .iter_mut()
            .fold((0.0, 0.0), |(left, right), grain| {
                let (l, r) = grain.advance();
                (left + l * INV_NUM_GRAINS, right + r * INV_NUM_GRAINS)
            })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Voice {
    pub grains: Grains,

    length: usize,
    env_increment: f32,
    env_position: f32,

    pub active: bool,
    pitch: f32,

    buffers_since_last_trigger: usize,
    buffers_between_triggers: usize,
}

impl Voice {
    pub fn new(table: &'static [f32]) -> Self {
        Self {
            grains: Grains::new(table),
            length: 0,
            env_increment: 0.0,
            env_position: 0.0,
            active: false,
            pitch: 440.0,
            buffers_since_last_trigger: 0,
            buffers_between_triggers: 4,
        }
    }

    fn trigger_grain(&mut self) {
        let _ = self.grains.activate(self.pitch).is_err();
    }

    pub fn update_grains(&mut self) {
        if self.buffers_since_last_trigger >= self.buffers_between_triggers {
            self.trigger_grain();
            self.buffers_since_last_trigger = 0;
        }
        self.buffers_since_last_trigger += 1;
    }

    /// called at frame rate
    fn advance(&mut self) -> (f32, f32) {
        let env = self.env();
        let (l, r) = self.grains.advance();
        (l * env, r * env)
    }

    fn env(&mut self) -> f32 {
        let env = if self.env_position > 0.5 {
            ((1.0 - self.env_position) * 4.0).min(1.0)
        } else {
            (self.env_position * 4.0).min(1.0)
        };

        self.env_position += self.env_increment;

        if self.env_position >= 1.0 {
            self.env_position = 0.0;
            self.active = false;
        }

        env
    }

    pub fn activate(&mut self, length: usize, pitch: f32) {
        self.length = length;
        self.env_increment = 1.0 / length as f32;
        self.env_position = 0.0;
        self.pitch = pitch;
        self.active = true;
    }

    pub fn process(&mut self, buffer: &mut Buffer) {
        for frame in buffer.frames_mut() {
            let (left, right) = self.advance();
            let mut frame_iter = frame.iter_mut();
            if let Some(left_out) = frame_iter.next() {
                *left_out += left;
            }
            if let Some(right_out) = frame_iter.next() {
                *right_out += right;
            }
        }
    }
}

pub type Voices = [Voice; NUM_VOICES];

pub struct Engine {
    pub voices: Voices,
    producer: Producer,
    buffers_since_last_trigger: usize,
    buffers_between_triggers: usize,
}

impl Engine {
    pub fn new(table: &'static [f32], producer: Producer) -> Self {
        Self {
            voices: [Voice::new(table); NUM_VOICES],
            producer,
            buffers_since_last_trigger: 0,
            buffers_between_triggers: 64,
        }
    }

    fn trigger(&mut self) {
        use rume::convert::pitch;
        let mut rng = thread_rng();
        let root = [-12.0, -12.0, 0.0, 0.0, 0.0, 7.0][rng.gen_range(0..=5)];
        let freqs = [
            pitch::from_midi(root + 60.0), // C4
            // pitch::from_midi(63.0), // D#4
            pitch::from_midi(root + 67.0), // G4
            // pitch::from_midi(70.0), // A#4
            // pitch::from_midi(72.0), // C5
            pitch::from_midi(root + 74.0), // D5
            pitch::from_midi(root + 79.0), // G5
        ];
        let mut inactive_voice_indices: Vec<usize> = Vec::new();
        for (i, voice) in self.voices.iter_mut().enumerate() {
            if !voice.active {
                inactive_voice_indices.push(i);
            }
        }
        if !inactive_voice_indices.is_empty() {
            let i = inactive_voice_indices[rng.gen_range(0..inactive_voice_indices.len())];
            let length = thread_rng().gen_range(4..24) * SAMPLE_RATE;
            self.voices[i].activate(length, freqs[i]);
        }
    }

    /// called at buffer rate
    fn update(&mut self) {
        if self.buffers_since_last_trigger >= self.buffers_between_triggers {
            self.trigger();
            self.buffers_since_last_trigger = 0;
        }
        self.buffers_since_last_trigger += 1;

        for voice in self.voices.iter_mut() {
            if voice.active {
                voice.update_grains();
            }
        }
    }

    pub fn process(&mut self, buffer: &mut Buffer) {
        self.update();
        for voice in self.voices.iter_mut() {
            if voice.active {
                voice.process(buffer);
            }
        }
        let _ = self.producer.enqueue(self.voices);
    }
}
