use cgmath::prelude::*;
use three::Object;
use std::sync::mpsc::*;
use std::thread;
use dasp_ring_buffer as ring_buffer;
use dasp_signal::{self as signal, Signal};
use dasp_signal::rms::SignalRms;

fn main() {
    let mut win = three::Window::new("Three-rs shapes example");
    let cam = win.factory.perspective_camera(75.0, 1.0 .. 50.0);
    cam.set_position([0.0, 0.0, 10.0]);

    let mbox = {
        let geometry = three::Geometry::cuboid(3.0, 2.0, 1.0);
        let material = three::material::Wireframe { color: 0x00FF00 };
        win.factory.mesh(geometry, material)
    };
    mbox.set_position([-3.0, -3.0, 0.0]);
    win.scene.add(&mbox);

    let mcyl = {
        let geometry = three::Geometry::cylinder(1.0, 2.0, 2.0, 5);
        let material = three::material::Wireframe { color: 0xFF0000 };
        win.factory.mesh(geometry, material)
    };
    mcyl.set_position([3.0, -3.0, 0.0]);
    win.scene.add(&mcyl);

    let msphere = {
        let geometry = three::Geometry::uv_sphere(2.0, 5, 5);
        let material = three::material::Wireframe { color: 0xFF0000 };
        win.factory.mesh(geometry, material)
    };
    msphere.set_position([-3.0, 3.0, 0.0]);
    win.scene.add(&msphere);

    // test removal from scene
    win.scene.remove(&mcyl);
    win.scene.remove(&mbox);
    win.scene.add(&mcyl);
    win.scene.add(&mbox);

    let mline = {
        let geometry = three::Geometry::with_vertices(vec![
            [-2.0, -1.0, 0.0].into(),
            [0.0, 1.0, 0.0].into(),
            [2.0, -1.0, 0.0].into(),
        ]);
        let material = three::material::Line { color: 0x0000FF };
        win.factory.mesh(geometry, material)
    };
    mline.set_position([3.0, 3.0, 0.0]);
    win.scene.add(&mline);

    let pa = portaudio::PortAudio::new().expect("Unable to init PortAudio"); 
    let mic_index = pa.default_input_device().expect("Unable to get default device");
    let mic = pa.device_info(mic_index).expect("unable to get mic info");

    let input_params = portaudio::StreamParameters::<f32>::new(mic_index, 1, true, mic.default_low_input_latency);
    let input_settings = portaudio::InputStreamSettings::new(input_params, mic.default_sample_rate, 256);

    let (sender, receiver) = channel();
    let callback = move |portaudio::InputStreamCallbackArgs {buffer, .. }| {
        match sender.send(buffer) {
            Ok(_) => portaudio::Continue, 
            Err(_) => portaudio::Complete
        }
    };
    let mut stream = pa.open_non_blocking_stream(input_settings, callback).expect("Unable to create stream"); 
    stream.start().expect("Unable to start stream");
    let ring_buffer = ring_buffer::Fixed::from([0.0 as f32; 4096]);

    let mut angle = cgmath::Rad::zero();
    while win.update() && !win.input.hit(three::KEY_ESCAPE) {
        // if let Some(diff) = win.input.timed(three::AXIS_LEFT_RIGHT) {
        let buffer = receiver.recv().expect("unable to get audio buffer");
        let v = buffer.to_vec();
        let signal = signal::from_iter(v.iter().cloned());
        let mut rms_signal = signal.rms(ring_buffer);
        let rms_signal = rms_signal.next();

        angle += cgmath::Rad(1.5 * rms_signal *100.0);
        let q = cgmath::Quaternion::from_angle_y(angle);
        mbox.set_orientation(q);
        mcyl.set_orientation(q);
        msphere.set_orientation(q);
        mline.set_orientation(q);
        // }
        win.render(&cam);
    }
}