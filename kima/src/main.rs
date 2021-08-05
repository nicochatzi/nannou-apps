use nannou::prelude::*;
use nannou::ui::prelude::*;
use nannou_audio as audio;

const SAMPLE_RATE: usize = 44_100;
const BUFFER_SIZE: usize = 512;
const NUM_CHANNELS: usize = 2;

struct Engine;

fn main() {
    nannou::app(model).update(update).simple_window(view).run();
}

struct Model {
    ui: Ui,
    stream: audio::Stream<Engine>,
}

fn model(app: &App) -> Model {
    app.set_loop_mode(LoopMode::rate_fps(SAMPLE_RATE as f64 / BUFFER_SIZE as f64));

    Model {
        ui: app.new_ui().build().unwrap(),
        stream: audio::Host::new()
            .new_output_stream(Engine)
            .sample_rate(SAMPLE_RATE as u32)
            .frames_per_buffer(BUFFER_SIZE)
            .channels(NUM_CHANNELS)
            .render(audio)
            .build()
            .unwrap(),
    }
}

fn audio(audio: &mut Engine, buffer: &mut audio::Buffer) {}

fn update(app: &App, model: &mut Model, _update: Update) {}

fn view(app: &App, model: &Model, frame: Frame) {
    let draw = app.draw();

    draw.background().color(DARKBLUE);
    draw.to_frame(app, &frame).unwrap();
}
