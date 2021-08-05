use lazy_static::lazy_static;
use nannou::prelude::*;
use nannou::ui::prelude::*;
use nannou_audio as audio;
use rand::prelude::*;
use rume::Processor;
use rume::Renderable;

fn main() {
    nannou::app(model).update(update).simple_window(view).run();
}

struct Model {
    ui: Ui,
    ids: Ids,
    tick: u32,
    lissa: Lissajous,
    stream: audio::Stream<Synth>,
}

const TABLE_SIZE: usize = 64;
const SAMPLE_RATE: f32 = 48_000.0;

const NUM_POINTS: usize = TABLE_SIZE * 4;
const SCALING: f32 = 0.25;

#[inline(always)]
pub fn lerp(x0: f32, x1: f32, w: f32) -> f32 {
    (1 as f32 - (w)) * x0 + (w * x1)
}

/// table must be power of 2
#[inline(always)]
pub fn filut(table: &[f32], index: f32) -> f32 {
    const WRAP_MASK: usize = TABLE_SIZE - 1;
    let index0: usize = index as usize;
    let index1: usize = (index0 + 1) & WRAP_MASK;
    let weight: f32 = index - index0 as f32;
    lerp(table[index0], table[index1], weight)
}

lazy_static! {
    pub static ref SIN_TABLE: [f32; TABLE_SIZE] = {
        let mut table = [0.0; TABLE_SIZE];
        for (i, value) in table.iter_mut().enumerate() {
            let phase = i as f32 / TABLE_SIZE as f32;
            *value = (2.0 * PI * phase).sin();
        }
        table
    };
    pub static ref FREQS: Vec<f32> = {
        let mut freqs = Vec::<f32>::new();
        freqs.push(36.0); // C
        freqs.push(38.0); // D
        freqs.push(39.0); // D#
        freqs.push(41.0); // F
        freqs.push(43.0); // G
        freqs.push(45.0); // A
        freqs.push(47.0); // B
        for freq in &mut freqs {
            *freq = 440.0 as f32 * (2.0 as f32).pow((*freq - 69.0) / 12.0) * 2.0
        }
        freqs
    };
    pub static ref RATIOS: Vec<f32> = {
        let mut ratios = Vec::<f32>::with_capacity(36);
        for i in 1..=6 {
            for j in 1..=6 {
                ratios.push(i as f32 / j as f32);
            }
        }
        ratios
    };
}

fn sin(freq: f32, t: f32, phase: f32) -> f32 {
    const SAMPLE_TIME: f32 = 1.0 as f32 / SAMPLE_RATE;
    let index = (TABLE_SIZE as f32 * freq * t * SAMPLE_TIME + phase) % TABLE_SIZE as f32;
    filut(&*SIN_TABLE, index)
}

struct SynthParams {
    freq_a: rume::InputStreamProducer,
    freq_b: rume::InputStreamProducer,
}

struct Synth {
    graph: rume::SignalChain,
    inputs: SynthParams,
    outputs: Vec<rume::OutputStreamConsumer>,
}

struct Lissajous {
    x_amp: f32,
    y_amp: f32,
    points: Vec<Point2>,
    delta: f32,
    phase: f32,
    freq_idx: f32,
    ratio_idx: f32,
    resolution: f32,
}

impl Lissajous {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            x_amp: width * SCALING,
            y_amp: height * SCALING,
            points: vec![Point2::default(); NUM_POINTS],
            delta: 3.14,
            phase: 0.0,
            freq_idx: 0.0,
            ratio_idx: 0.0,
            resolution: 0.01,
        }
    }

    pub fn compute(&mut self) {
        let (x_freq, y_freq) = self.freqs();
        for i in 0..NUM_POINTS {
            self.phase += i as f32 * self.resolution;
            self.points[i].x = self.x_amp * sin(x_freq, self.phase, self.delta);
            self.points[i].y = self.y_amp * sin(y_freq, self.phase, 0.0);
        }
    }

    pub fn freqs(&self) -> (f32, f32) {
        let compute_idx = |raw_idx: f32, max_length: usize| -> f32 {
            const SKEW: f32 = 10.0;
            ((raw_idx as usize) as f32 + (raw_idx % 1.0).pow(SKEW)) % max_length as f32
        };
        let freq_idx = compute_idx(self.freq_idx, FREQS.len());
        let ratio_idx = compute_idx(self.ratio_idx, RATIOS.len());
        let ratio = filut(&*RATIOS, ratio_idx);
        let mut freq = filut(&*FREQS, freq_idx);
        if ratio >= 3.0 {
            freq /= 2.0;
        }
        (freq, freq * filut(&*RATIOS, ratio_idx))
    }
}

widget_ids! {
    struct Ids {
        delta,
        tick,
        x_freq,
        y_freq,
        freq_idx,
        ratio_idx,
        resolution,
    }
}

fn model(app: &App) -> Model {
    app.set_loop_mode(LoopMode::RefreshSync);

    let mut ui = app.new_ui().build().unwrap();
    let ids = Ids::new(ui.widget_id_generator());
    let lissa = Lissajous::new(ui.win_w.clone() as f32, ui.win_h.clone() as f32);

    let (freq_a_prod, freq_a_con) = rume::input!(FREQ_A_ENDPOINT);
    let (freq_b_prod, freq_b_con) = rume::input!(FREQ_B_ENDPOINT);
    let (out_r_prod, out_r_con) = rume::output!(OUT_R_ENDPOINT);
    let (out_l_prod, out_l_con) = rume::output!(OUT_L_ENDPOINT);

    let graph = rume::graph! {
        endpoints: {
            freq_a: rume::InputEndpoint::new(freq_a_con),
            freq_b: rume::InputEndpoint::new(freq_b_con),
            out_r: rume::OutputEndpoint::new(out_r_prod),
            out_l: rume::OutputEndpoint::new(out_l_prod),
        },
        processors: {
            sine_a: rume::Sine::default(),
            sine_b: rume::Sine::default(),
            amp: rume::Value::new(0.1),
        },
        connections: {
            freq_a.output   -> sine_a.input.0,
            freq_b.output   -> sine_b.input.0,
            amp.output      -> sine_a.input.1,
            amp.output      -> sine_b.input.1,
            sine_a.output   -> out_r.input,
            sine_b.output   -> out_l.input,
        }
    };

    let synth = Synth {
        graph,
        inputs: SynthParams {
            freq_a: freq_a_prod,
            freq_b: freq_b_prod,
        },
        outputs: vec![out_l_con, out_r_con],
    };

    let audio_host = audio::Host::new();
    let stream = audio_host
        .new_output_stream(synth)
        .render(audio)
        .build()
        .unwrap();

    Model {
        ui,
        tick: 0,
        ids,
        lissa,
        stream,
    }
}

fn update(_app: &App, model: &mut Model, update: Update) {
    let ui = &mut model.ui.set_widgets();

    fn slider(val: f32, min: f32, max: f32) -> widget::Slider<'static, f32> {
        widget::Slider::new(val, min, max)
            .w_h(200.0, 30.0)
            .label_font_size(15)
            .rgb(0.0, 0.5, 0.0)
            .label_rgb(0.0, 0.0, 0.0)
            .border(0.0)
    }

    for value in slider(model.lissa.delta as f32, 0.0, TABLE_SIZE as f32)
        .top_left_with_margin(20.0)
        .label("δ")
        .set(model.ids.delta, ui)
    {
        model.lissa.delta = value;
    }

    for value in slider(model.lissa.resolution as f32, 0.05, 0.001)
        .down(20.0)
        .label("γ")
        .set(model.ids.resolution, ui)
    {
        model.lissa.resolution = value;
    }

    let time = update.since_start.as_millis() as f32 / 100.0;
    model.tick += (time % 2.0) as u32;

    let mut rng = rand::thread_rng();

    if model.tick as f32 > rng.gen_range(1.0, 300.0) {
        if rand::random() {
            model.lissa.ratio_idx = rng.gen_range(0.0, (RATIOS.len() - 1) as f32);
        }
        if rand::random() {
            let new_freq = rng.gen_range(0.0, (FREQS.len() - 1) as f32);
            if (new_freq % 1.0) as u8 != (model.lissa.freq_idx % 1.0) as u8 {
                model.lissa.freq_idx = new_freq;
            }
        }
        model.tick = 0;
    }

    model.lissa.compute();
    let (x_freq, y_freq) = model.lissa.freqs();
    let _ = model.stream.send(move |synth: &mut Synth| {
        synth.inputs.freq_a.enqueue(x_freq).unwrap();
        synth.inputs.freq_b.enqueue(y_freq).unwrap();
    });
}

fn audio(synth: &mut Synth, buffer: &mut audio::Buffer) {
    let sample_rate = buffer.sample_rate() as u32;
    let buffer_size = buffer.len_frames() as usize;

    synth.graph.prepare(sample_rate.into());
    synth.graph.render(buffer_size);

    for frame in buffer.frames_mut() {
        for (i, channel) in frame.iter_mut().enumerate() {
            *channel = synth.outputs[i].dequeue().unwrap();
        }
    }
}

fn view(app: &App, model: &Model, frame: Frame) {
    let draw = app.draw();

    draw.background().rgb(0.04, 0.04, 0.04);

    draw.polyline()
        .weight(1.0)
        .points(model.lissa.points.clone())
        .rgb(0.0, 1.0, 0.0);

    draw.to_frame(app, &frame).unwrap();
    model.ui.draw_to_frame(app, &frame).unwrap();
}
