#![allow(dead_code)]

use dsp::NUM_GRAINS;
use nannou::prelude::*;
use nannou::ui::prelude::*;
use nannou_audio as audio;

mod dsp;

lazy_static::lazy_static! {
    pub static ref SAMPLES: Vec<f32> = {
        use nannou_audio::sample::conv;
        hound::WavReader::open(&format!("{}/res/old.wav", env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .samples::<i16>()
            .map(|x| conv::i16::to_f32(x.unwrap()))
            .collect()
    };
}

fn main() {
    // wav::to_file();
    nannou::app(model).update(update).simple_window(view).run();
}

#[derive(Clone, Default)]
struct Polygon {
    active: bool,
    vertices: Vec<Point2>,
    color: Rgba8,
}

struct Model {
    ui: Ui,
    polygons: Vec<Polygon>,
    consumer: dsp::Consumer,
    voices: dsp::Voices,
    stream: audio::Stream<dsp::Engine>,
}

fn model(app: &App) -> Model {
    app.set_loop_mode(LoopMode::rate_fps(
        dsp::SAMPLE_RATE as f64 / dsp::BUFFER_SIZE as f64,
    ));

    let (producer, consumer) = {
        use heapless::{i, spsc};
        static mut QUEUE: dsp::Queue = spsc::Queue(i::Queue::new());
        unsafe { QUEUE.split() }
    };

    // Initialise the state that we want to live on the audio thread.
    Model {
        ui: app.new_ui().build().unwrap(),
        consumer,
        polygons: (0..dsp::NUM_GRAINS * dsp::NUM_VOICES)
            .map(|_| Polygon::default())
            .collect(),
        voices: [dsp::Voice::new(&SAMPLES); dsp::NUM_VOICES],
        stream: audio::Host::new()
            .new_output_stream(dsp::Engine::new(&SAMPLES, producer))
            .sample_rate(dsp::SAMPLE_RATE as u32)
            .frames_per_buffer(dsp::BUFFER_SIZE)
            .channels(dsp::NUM_CHANNELS)
            .render(audio)
            .build()
            .unwrap(),
    }
}

fn audio(audio: &mut dsp::Engine, buffer: &mut audio::Buffer) {
    audio.process(buffer);
}

fn update(app: &App, model: &mut Model, _update: Update) {
    const TWO_PI: f32 = 2.0 * PI;
    const RESOLUTION: usize = dsp::BUFFER_SIZE;
    const INV_RESOLUTION: f32 = 1.0 / RESOLUTION as f32;
    const COLORS: [Rgb8; 4] = [LIGHTCORAL, LIGHTSALMON, LIGHTSEAGREEN, DARKTURQUOISE];

    let win = app.window_rect();

    if let Some(voices) = model.consumer.dequeue() {
        for (i, voice) in voices.clone().iter_mut().enumerate() {
            for (j, grain) in voice.grains.grains.iter_mut().enumerate() {
                let mut polygon = &mut model.polygons[i * NUM_GRAINS + j];

                polygon.active = grain.active && voice.active;
                if !polygon.active {
                    continue;
                }

                let x = win.w() * 0.40 * ((grain.pan * 2.0) - 1.0);
                let y = match i {
                    0 => win.h() * -0.30,
                    1 => win.h() * -0.10,
                    2 => win.h() * 0.10,
                    3 => win.h() * 0.30,
                    _ => win.h() * 0.0,
                };

                let mut rms = 0.0;
                polygon.vertices = (0..RESOLUTION)
                    .step_by(8)
                    .map(|vertex| {
                        let r = {
                            let sample = grain.advance();
                            let sample = sample.0 + sample.1;
                            let vol = grain.volume * 100.0;
                            rms += sample.pow(2);
                            map_range(sample, 0.0, 1.0, vol * 0.75, vol)
                        };
                        let theta = TWO_PI * vertex as f32 * INV_RESOLUTION;
                        pt2(x + r * theta.cos(), y + r * theta.sin())
                    })
                    .collect();

                polygon.color = {
                    let mut c = Rgba8::from(COLORS[i]);
                    c.alpha = ((rms * INV_RESOLUTION).sqrt() * 1024.0) as u8;
                    c
                };
            }
        }
    }
}

fn view(app: &App, model: &Model, frame: Frame) {
    let draw = app.draw();

    draw.background().color(BISQUE);

    for polygon in model.polygons.iter() {
        if polygon.active {
            let polygon = polygon.clone();
            draw.polygon().points(polygon.vertices).color(polygon.color);
            // draw.polyline()
            //     .weight(1.0)
            //     .points_closed(polygon.vertices)
            //     .color(polygon.color);
        }
    }

    draw.to_frame(app, &frame).unwrap();
}
