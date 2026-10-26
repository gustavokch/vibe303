use nih_plug::prelude::*;
use plugin303::standalone_args::with_detected_sample_rate;
use plugin303::Plugin303;

/// Sample rate of the output device that nih-plug's CoreAudio backend opens.
///
/// cpal's CoreAudio backend does not switch the device's sample rate. At a stream rate above
/// the device rate, CoreAudio delivers more frames per callback than nih-plug's period size
/// (e.g. 558 > 512 at 48 kHz on a 44.1 kHz device), which panics the audio thread.
#[cfg(target_os = "macos")]
fn output_device_sample_rate(args: &[String]) -> Option<u32> {
    use cpal::traits::{DeviceTrait, HostTrait};
    use plugin303::standalone_args::output_device;

    let host = cpal::default_host();
    // Same lookup as nih-plug's `CpalMidir::new`.
    let device = match output_device(args) {
        Some(name) => host
            .output_devices()
            .ok()?
            .find(|d| d.name().is_ok_and(|n| n == name))?,
        None => host.default_output_device()?,
    };
    Some(device.default_output_config().ok()?.sample_rate().0)
}

#[cfg(not(target_os = "macos"))]
fn output_device_sample_rate(_args: &[String]) -> Option<u32> {
    None
}

fn main() {
    let args = with_detected_sample_rate(std::env::args().collect(), output_device_sample_rate);
    nih_export_standalone_with_args::<Plugin303, _>(args);
}
