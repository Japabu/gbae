use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use gbae::system::apu::SAMPLE_RATE;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

const MAX_QUEUED_FRAMES: usize = SAMPLE_RATE as usize / 5;

pub struct Audio {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<i16>>>,
    volume: Arc<AtomicU8>,
    device_rate: u32,
    resample_position: f64,
}

impl Audio {
    pub fn new(volume: u8) -> Option<Audio> {
        let device = cpal::default_host().default_output_device()?;
        let config = device.default_output_config().ok()?;
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let volume = Arc::new(AtomicU8::new(volume));
        let channels = config.channels() as usize;
        let device_rate = config.sample_rate();
        let stream_config: cpal::StreamConfig = config.clone().into();
        let stream = match config.sample_format() {
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, stream_config, queue.clone(), volume.clone(), channels),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, stream_config, queue.clone(), volume.clone(), channels),
            _ => build_stream::<f32>(&device, stream_config, queue.clone(), volume.clone(), channels),
        }
        .map_err(|error| eprintln!("Could not open audio output: {}", error))
        .ok()?;
        stream.play().ok()?;
        Some(Audio {
            _stream: stream,
            queue,
            volume,
            device_rate,
            resample_position: 0.0,
        })
    }

    pub fn set_volume(&self, volume: u8) {
        self.volume.store(volume, Ordering::Relaxed);
    }

    pub fn push(&mut self, samples: &[i16]) {
        let mut queue = self.queue.lock().unwrap();
        if queue.len() / 2 > MAX_QUEUED_FRAMES {
            return;
        }
        let frames = samples.len() / 2;
        if self.device_rate == SAMPLE_RATE {
            queue.extend(samples.iter().copied());
        } else {
            let step = SAMPLE_RATE as f64 / self.device_rate as f64;
            while (self.resample_position as usize) < frames {
                let frame = self.resample_position as usize;
                queue.push_back(samples[frame * 2]);
                queue.push_back(samples[frame * 2 + 1]);
                self.resample_position += step;
            }
            self.resample_position -= frames as f64;
        }
    }

    pub fn clear(&self) {
        self.queue.lock().unwrap().clear();
    }
}

fn build_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    queue: Arc<Mutex<VecDeque<i16>>>,
    volume: Arc<AtomicU8>,
    channels: usize,
) -> Result<cpal::Stream, cpal::Error> {
    device.build_output_stream(
        config,
        move |output: &mut [T], _| {
            let mut queue = queue.lock().unwrap();
            let gain = volume.load(Ordering::Relaxed) as f32 / 100.0;
            for frame in output.chunks_mut(channels) {
                let left = queue.pop_front().unwrap_or(0) as f32 / 32768.0 * gain;
                let right = queue.pop_front().unwrap_or(0) as f32 / 32768.0 * gain;
                for (channel, sample) in frame.iter_mut().enumerate() {
                    *sample = T::from_sample(if channel % 2 == 0 { left } else { right });
                }
            }
        },
        |error| eprintln!("Audio stream error: {}", error),
        None,
    )
}
