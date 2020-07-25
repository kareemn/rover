use cgmath::prelude::*;
use three::Object;
use std::sync::mpsc::*;
use std::thread;
use dasp_ring_buffer as ring_buffer;
use dasp_signal::{self as signal, Signal};
use dasp_signal::rms::SignalRms;
use minifb::{Key, Window, WindowOptions};

const WIDTH: usize = 100;
const HEIGHT: usize = 1024;

fn from_u8_rgb(r: u8, g: u8, b: u8) -> u32 {
    let (r, g, b) = (r as u32, g as u32, b as u32);
    (r << 16) | (g << 8) | b
}

fn main() {
    let mut win = three::Window::new("Three-rs shapes example");
    let cam = win.factory.perspective_camera(75.0, 1.0 .. 50.0);
    cam.set_position([0.0, 0.0, 10.0]);

    // let hemisphere_light = win.factory.hemisphere_light(0xffffff, 0x8080ff, 0.5);
    let ambient_light = win.factory.ambient_light(0xffffffff, 0.5);
    let point_light = win.factory.point_light(0xffffff, 0.7);
    point_light.set_position([15.0, 35.0, 35.0]);

    // let mut dir_light = win.factory.directional_light(0xffffff, 0.9);
    // dir_light.look_at([15.0, 35.0, 35.0], [0.0, 0.0, 2.0], None);
    // let shadow_map = win.factory.shadow_map(1024, 1024);
    // let _debug_shadow = win.renderer
    //     .debug_shadow_quad(&shadow_map, 1, [10, 10], [256, 256]);
    // dir_light.set_shadow(shadow_map, 40.0, 1.0 .. 200.0);

    let lights: [&three::object::Base; 2] = [
        // hemisphere_light.as_ref(),
        ambient_light.as_ref(),
        point_light.as_ref(),
        // dir_light.as_ref(),
    ];
    for l in &lights {
        l.set_visible(true);
        win.scene.add(l);
    }

    let mbox = {
        let geometry = three::Geometry::cuboid(3.0, 2.0, 1.0);
        let material = three::material::Phong { color: 0x00FF00, glossiness: 80.0, };
        win.factory.mesh(geometry, material)
    };
    mbox.set_position([-3.0, -3.0, 0.0]);
    // win.scene.add(&mbox);

    let mcyl = {
        let geometry = three::Geometry::cylinder(1.0, 2.0, 2.0, 5);
        let material = three::material::Phong { color: 0xFF0000, glossiness: 80.0, };
        win.factory.mesh(geometry, material)
    };
    mcyl.set_position([3.0, -3.0, 0.0]);
    // win.scene.add(&mcyl);

    let msphere = {
        let geometry = three::Geometry::uv_sphere(2.0, 5, 5);
        let material = three::material::Phong { color: 0xFF0000, glossiness: 80.0, };
        win.factory.mesh(geometry, material)
    };
    msphere.set_position([-3.0, 3.0, 0.0]);
    // win.scene.add(&msphere);

    // win.scene.add(&mcyl);
    // win.scene.add(&mbox);

    let mline = {
        let geometry = three::Geometry::with_vertices(vec![
            [-2.0, -1.0, 0.0].into(),
            [0.0, 1.0, 0.0].into(),
            [2.0, -1.0, 0.0].into(),
        ]);
        let material = three::material::Phong { color: 0x0000FF , glossiness: 80.0,};
        win.factory.mesh(geometry, material)
    };
    mline.set_position([3.0, 3.0, 0.0]);
    // win.scene.add(&mline);

    let sphere_geometry = three::Geometry::uv_sphere(2.0, 32, 32);
    let mut dynamic = {
        let material = three::material::Wireframe{
            color: 0xFFFFFF,
        };
        // let material = three::material::Pbr{
        //     base_color_factor: 0xffA0A0,
        //     base_color_alpha: 0.3,
        //     metallic_factor: 1.0,
        //     roughness_factor: 0.5,
        //     occlusion_strength: 0.5,
        //     emissive_factor: three::color::BLACK,
        //     normal_scale: 1.0,
        //     base_color_map: None,
        //     normal_map: None,
        //     emissive_map: None,
        //     metallic_roughness_map: None,
        //     occlusion_map: None,
        // };
        // let material = three::material::Phong {
        //     color: 0xffA0A0,
        //     glossiness: 80.0,
        // };
        // let material = three::material::Lambert {
        //     color: 0xA0ffA0,
        //     flat: false,
        // };
        win.factory.mesh_dynamic(sphere_geometry, material)
    };
    dynamic.set_position([0.0, 0.0, 0.0]);
    win.scene.add(&dynamic);

    let mut frame_buffer: Vec<u32> = vec![0; WIDTH * HEIGHT];

    let mut window = Window::new(
        "Test - ESC to exit",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )
    .unwrap_or_else(|e| {
        panic!("{}", e);
    });

    // Limit to max ~60 fps update rate
    window.limit_update_rate(Some(std::time::Duration::from_micros(16600)));

    println!("vertex count: {}", dynamic.vertex_count());

    let pa = portaudio::PortAudio::new().expect("Unable to init PortAudio"); 
    let mic_index = pa.default_input_device().expect("Unable to get default device");
    let mic = pa.device_info(mic_index).expect("unable to get mic info");

    let input_params = portaudio::StreamParameters::<i16>::new(mic_index, 1, true, mic.default_low_input_latency);
    let input_settings = portaudio::InputStreamSettings::new(input_params, mic.default_sample_rate, 512);

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
        let mut spectrograph = sonogram::SpecOptionsBuilder::new(1, 1024)
            .load_data_from_memory(v, mic.default_sample_rate as u32)
            .build();

        // Compute the spectrogram giving the number of bins and the window overlap.
        spectrograph.compute(1024, 0.1);
        
        let bins = spectrograph.create_in_memory(false);
        spectrograph.save_as_png(&std::path::Path::new("test.png"), true).expect("didnt work");
        // println!("{}", rms_signal);
        let vertex_count = dynamic.vertex_count();
        {
            let mut vmap = win.factory.map_vertices(&mut dynamic);
            for i in 0..vertex_count {
                let v_i = (i + 256+512) % vertex_count;
                let mut ratio = (bins[i]-4.0) / 4.0;
                if ratio < 0.0 {
                    ratio = 0.0 ;
                }
                for j in 0..WIDTH {
                    frame_buffer[v_i*WIDTH + j] = from_u8_rgb((ratio * 255 as f32) as u8, 0, 0);
                }
                let dir = cgmath::Vector4::from(vmap[v_i].pos).truncate();
                let pos = cgmath::Point3::from_vec(1.0 * dir);
                ratio += 1.0;
                vmap[v_i].pos = [pos.x, pos.y, pos.z, 1.0/ratio.powf(3.0)];
            }
        }
        window
            .update_with_buffer(&frame_buffer, WIDTH, HEIGHT)
            .unwrap();

        angle += cgmath::Rad(1.5 * rms_signal *100.0);
        // point_light.set_position([1.0* rms_signal *100000.0, 35.0, 35.0]);
        let q1 = cgmath::Quaternion::from_angle_y(angle);
        let q2= cgmath::Quaternion::from_angle_y(angle);
        let q3 = cgmath::Quaternion::from_angle_y(angle);
        let q4 = cgmath::Quaternion::from_angle_y(angle);

        mbox.set_orientation(q1);
        mcyl.set_orientation(q2);
        msphere.set_orientation(q3);
        mline.set_orientation(q4);
        // }
        win.render(&cam);
    }
}