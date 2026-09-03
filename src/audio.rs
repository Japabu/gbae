use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

const MAX_QUEUED_FRACTION_OF_SECOND: usize = 5;

pub struct Audio {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<i16>>>,
    volume: Arc<AtomicU8>,
    sample_rate: u32,
}

impl Audio {
    pub fn new(volume: u8, wake: impl Fn() + Send + 'static) -> Option<Audio> {
        let device = cpal::default_host().default_output_device()?;
        let config = device.default_output_config().ok()?;
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let volume = Arc::new(AtomicU8::new(volume));
        let channels = config.channels() as usize;
        let sample_rate = config.sample_rate();
        let stream_config: cpal::StreamConfig = config.into();
        let stream = match config.sample_format() {
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, stream_config, queue.clone(), volume.clone(), channels, wake),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, stream_config, queue.clone(), volume.clone(), channels, wake),
            _ => build_stream::<f32>(&device, stream_config, queue.clone(), volume.clone(), channels, wake),
        }
        .map_err(|error| eprintln!("gbae: cannot open audio output: {}", error))
        .ok()?;
        stream.play().ok()?;
        Some(Audio {
            _stream: stream,
            queue,
            volume,
            sample_rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn set_volume(&self, volume: u8) {
        self.volume.store(volume, Ordering::Relaxed);
    }

    pub fn queued_frames(&self) -> usize {
        self.queue.lock().unwrap().len() / 2
    }

    pub fn push(&self, samples: &[i16]) {
        let mut queue = self.queue.lock().unwrap();
        if queue.len() / 2 > self.sample_rate as usize / MAX_QUEUED_FRACTION_OF_SECOND {
            return;
        }
        queue.extend(samples.iter().copied());
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
    wake: impl Fn() + Send + 'static,
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
            drop(queue);
            wake();
        },
        |error| eprintln!("gbae: audio stream error: {}", error),
        None,
    )
}
