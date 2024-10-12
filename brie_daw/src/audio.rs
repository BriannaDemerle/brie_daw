use bincode::{serialize, ErrorKind};
use rodio::{buffer::SamplesBuffer, OutputStream, PlayError, Source};
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Write, ops::Range, thread::JoinHandle, time::Duration};

pub type PlayerThread = JoinHandle<Result<(), PlayError>>;

pub fn iter_mask(range: Range<usize>) -> impl Iterator<Item = bool> {
    (0..range.end).map(move |n| range.contains(&n))
}

fn apply_conditional_map<I, O, F>(into_iterator: &I, range: Range<usize>, f: F) -> O
where
    I: IntoIterator<Item = i16> + Clone,
    O: FromIterator<i16>,
    F: Fn(i16) -> i16,
{
    into_iterator
        .clone()
        .into_iter()
        .zip(iter_mask(range))
        .map(|(m, b)| if b { f(m) } else { m })
        .collect()
}

pub trait ConditionalMappable {
    fn conditional_map<F: Fn(i16) -> i16>(&mut self, range: Range<usize>, f: F) -> Self;
}

#[derive(Debug)]
pub enum ExportError {
    BincodeError(Box<ErrorKind>),
    FileError(std::io::Error),
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct WavSettings {
    channel_count: u16,
    sample_rate: u32,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct SoundData {
    wav_settings: WavSettings,
    samples: Vec<i16>,
}

impl SoundData {
    pub fn new(wav_settings: WavSettings) -> SoundData {
        SoundData {
            wav_settings,
            samples: vec![],
        }
    }

    pub fn set_sample(&mut self, index: usize, channel: usize, new_sample: i16) -> bool {
        let index: usize = index * self.wav_settings.channel_count as usize + channel;
        let maybe_sample: Option<&mut i16> = self.samples.get_mut(index);

        if let Some(sample) = maybe_sample {
            *sample = new_sample;
            true
        } else {
            false
        }
    }

    pub fn play_sound(&self) -> PlayerThread {
        let buffer: Vec<f32> = self
            .samples
            .iter()
            .map(|x| *x as f32 / (i16::MAX as f32))
            .collect();
        let sample_rate: u32 = self.wav_settings.sample_rate;

        std::thread::spawn(move || -> Result<(), PlayError> {
            let (_stream, stream_handle) =
                OutputStream::try_default().expect("Oops! Could not get device to play audio to!");
            let samples_buffer: SamplesBuffer<f32> = SamplesBuffer::new(1, sample_rate, buffer);
            let duration: Duration = samples_buffer.total_duration().expect("no duration found");
            stream_handle.play_raw(samples_buffer)?;
            std::thread::sleep(duration);
            Ok(())
        })
    }
}

impl ConditionalMappable for SoundData {
    fn conditional_map<F: Fn(i16) -> i16>(&mut self, range: Range<usize>, f: F) -> Self {
        let wav_settings = self.wav_settings;
        SoundData {
            samples: apply_conditional_map(&self.samples, range, f),
            wav_settings,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct WavHeader {
    riff: [u8; 4],
    file_size: u32,
    wave: [u8; 4],
    fmt: [u8; 4],
    format_size: u32,
    format_type: u16,
    channel_count: u16,
    sample_rate: u32,
    byte_rate: u32,
    bytes_per_chunk: u16,
    bits_per_sample: u16,
    data: [u8; 4],
    data_size: u32,
}

impl WavHeader {
    pub const RIFF: [u8; 4] = *b"RIFF";
    pub const WAVE: [u8; 4] = *b"WAVE";
    pub const FMT: [u8; 4] = *b"fmt ";
    pub const DATA: [u8; 4] = *b"data";
    pub const FORMAT_SIZE: u32 = 16;
    pub const FORMAT_TYPE: u16 = 1;
    pub const HEADER_SIZE: u32 = 44;
    pub const BYTES_PER_SAMPLE: u16 = 2;

    pub fn new(file_size: u32, wav_settings: WavSettings) -> WavHeader {
        WavHeader {
            riff: Self::RIFF,
            file_size,
            wave: Self::WAVE,
            fmt: Self::FMT,
            format_size: Self::FORMAT_SIZE,
            format_type: Self::FORMAT_TYPE,
            channel_count: wav_settings.channel_count,
            sample_rate: wav_settings.sample_rate,
            byte_rate: wav_settings.sample_rate
                * Self::BYTES_PER_SAMPLE as u32
                * wav_settings.channel_count as u32,
            bytes_per_chunk: Self::BYTES_PER_SAMPLE * wav_settings.channel_count,
            bits_per_sample: Self::BYTES_PER_SAMPLE * 8,
            data: Self::DATA,
            data_size: file_size - Self::HEADER_SIZE,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct WavFile {
    header: WavHeader,
    samples: Vec<i16>,
}

impl WavFile {
    pub fn new(sound_data: SoundData) -> WavFile {
        WavFile {
            header: WavHeader::new(sound_data.samples.len() as u32, sound_data.wav_settings),
            samples: sound_data.samples,
        }
    }

    pub fn export(&self, file: &mut File) -> Result<(), ExportError> {
        let bytes: Vec<u8> = serialize(self).map_err(|e| ExportError::BincodeError(e))?;
        file.write_all(&bytes)
            .map_err(|e| ExportError::FileError(e))?;
        Ok(())
    }
}
