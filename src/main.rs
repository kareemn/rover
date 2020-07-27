use cgmath::prelude::*;
use three::Object;
use std::sync::mpsc::*;
use minifb::{ Window, WindowOptions};
use rand::thread_rng;
use rand::seq::SliceRandom;

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

    let ambient_light = win.factory.ambient_light(0xffffffff, 0.5);
    let point_light = win.factory.point_light(0xffffff, 0.7);
    point_light.set_position([15.0, 35.0, 35.0]);

    let lights: [&three::object::Base; 2] = [
        ambient_light.as_ref(),
        point_light.as_ref(),
    ];
    for l in &lights {
        l.set_visible(true);
        win.scene.add(l);
    }

    let sphere_geometry = three::Geometry::uv_sphere(2.0, 32, 32);
    let mut dynamic = {
        let material = three::material::Wireframe{
            color: 0xFFFFFF,
        };
        win.factory.mesh_dynamic(sphere_geometry, material)
    };
    dynamic.set_position([0.0, 0.0, 0.0]);
    win.scene.add(&dynamic);


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

    let mut freq_frame_buffer: Vec<u32> = vec![0; WIDTH * HEIGHT];
    let mut freq_window = Window::new(
        "Test - ESC to exit",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )
    .unwrap_or_else(|e| {
        panic!("{}", e);
    });
    // // Limit to max ~60 fps update rate
    freq_window.limit_update_rate(Some(std::time::Duration::from_micros(16600)));

    let mut gan_frame_buffer: Vec<u32> = vec![0; 512 * 512];

    let mut gan_window = Window::new(
        "gan window",
        512,
        512,
        WindowOptions::default(),
    )
    .unwrap_or_else(|e| {
        panic!("{}", e);
    });

    // Limit to max ~60 fps update rate
    gan_window.limit_update_rate(Some(std::time::Duration::from_micros(16600)));
    let model = tch::CModule::load("./traced_portrait_dgan.pt").expect("could not load torch model");

    let mut noise: Vec<f32> = vec![0.0; 100];

    let mut iv: Vec<usize> = vec![0; 100];
    for i in 0..100 {
        iv[i] = i;
    }
    let mut direction: Vec<usize> = vec![0; 100];

    let indicies = iv.as_mut_slice();

    while win.update() && !win.input.hit(three::KEY_ESCAPE) {
        let mut rng = thread_rng();
        indicies.shuffle(&mut rng);

        // if let Some(diff) = win.input.timed(three::AXIS_LEFT_RIGHT) {
        let buffer = receiver.recv().expect("unable to get audio buffer");
        let v = buffer.to_vec();
        let mut spectrograph = sonogram::SpecOptionsBuilder::new(1, 1024)
            .load_data_from_memory(v, mic.default_sample_rate as u32)
            .build();

        // Compute the spectrogram giving the number of bins and the window overlap.
        spectrograph.compute(1024, 0.1);
        
        let bins = spectrograph.create_in_memory(false);
        let vertex_count = dynamic.vertex_count();
        {
            let mut vmap = win.factory.map_vertices(&mut dynamic);
            for i in 0..vertex_count {
                let v_i = (i + 256+512) % vertex_count;
                let mut ratio = (bins[i]-4.0) / 4.0;
                if ratio < 0.0 {
                    ratio = 0.0 ;
                }
                if i % 10 == 0 {
                    let random_index = indicies[i/10];
                    noise[random_index] = noise[random_index]/1.2; // (ratio - 0.1)* 10.0;
                    if direction[random_index] == 0 {
                        noise[random_index] = noise[random_index] - ratio*6.0;
                    } else {
                        noise[random_index] = noise[random_index] + ratio*6.0;
                    }
                    if noise[random_index] > 2.0 {
                        direction[random_index] = 0;
                    } else if noise[random_index] < -2.0 {
                        direction[random_index] = 1;
                    }
                    // println!("{:?}", noise);
                    
                }
                for j in 0..WIDTH {
                    freq_frame_buffer[v_i*WIDTH + j] = from_u8_rgb((ratio * 255 as f32) as u8, 0, 0);
                }
                let dir = cgmath::Vector4::from(vmap[v_i].pos).truncate();
                let pos = cgmath::Point3::from_vec(1.0 * dir);
                ratio += 1.0;
                vmap[v_i].pos = [pos.x, pos.y, pos.z, 1.0/ratio.powf(3.0)];
            }
        }


        run_gan_and_fill_buffer(&noise, &model, &mut gan_frame_buffer);

        gan_window
            .update_with_buffer(&gan_frame_buffer, 512, 512)
            .unwrap();

        freq_window
            .update_with_buffer(&freq_frame_buffer, WIDTH, HEIGHT)
            .unwrap();


        win.render(&cam);
    }
}

fn run_gan_and_fill_buffer(noise: &Vec<f32>, gan_model: &tch::CModule, gan_frame_buffer: &mut Vec<u32>) {
    let mut noise_tensor = tch::Tensor::of_slice(noise);
    noise_tensor = noise_tensor.unsqueeze(0);
    noise_tensor = noise_tensor.unsqueeze(2);
    noise_tensor = noise_tensor.unsqueeze(3);
    // let t = tch::Tensor::randn(&[1,100,1,1], (tch::Kind::Float, tch::Device::Cpu));
    let output = gan_model.forward_ts(&[noise_tensor]).expect("could not do forward pass").squeeze().permute(&[1, 2, 0]);
    let tensor_vec = Vec::<f64>::from((output + 1) * 0.5);
    for i in 0..128 {
        for j in 0..128 {
            let pixel = from_u8_rgb(
                (tensor_vec[i*128*3+j*3+0]* 255 as f64) as u8,
                (tensor_vec[i*128*3+j*3+1]* 255 as f64) as u8,
                (tensor_vec[i*128*3+j*3+2]* 255 as f64) as u8);

            gan_frame_buffer[(i*512*4 + j*4) as usize] = pixel;
            gan_frame_buffer[(i*512*4 + j*4+1) as usize] = pixel;
            gan_frame_buffer[(i*512*4 + j*4+2) as usize] = pixel;
            gan_frame_buffer[(i*512*4 + j*4+3) as usize] = pixel;

            gan_frame_buffer[(i*512*4+(512*1) + j*4) as usize] = pixel;
            gan_frame_buffer[(i*512*4+(512*1) + j*4+1) as usize] = pixel;
            gan_frame_buffer[(i*512*4+(512*1) + j*4+2) as usize] = pixel;
            gan_frame_buffer[(i*512*4+(512*1) + j*4+3) as usize] = pixel;

            gan_frame_buffer[(i*512*4+(512*2) + j*4) as usize] = pixel;
            gan_frame_buffer[(i*512*4+(512*2) + j*4+1) as usize] = pixel;
            gan_frame_buffer[(i*512*4+(512*2) + j*4+2) as usize] = pixel;
            gan_frame_buffer[(i*512*4+(512*2) + j*4+3) as usize] = pixel;
            
            gan_frame_buffer[(i*512*4+(512*3) + j*4) as usize] = pixel;
            gan_frame_buffer[(i*512*4+(512*3) + j*4+1) as usize] = pixel;
            gan_frame_buffer[(i*512*4+(512*3) + j*4+2) as usize] = pixel;
            gan_frame_buffer[(i*512*4+(512*3) + j*4+3) as usize] = pixel;
        }
    }
}