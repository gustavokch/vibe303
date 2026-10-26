use nih_plug::prelude::*;
use plugin303::Plugin303;

fn has_sample_rate_arg(args: &[String]) -> bool {
    args.iter().skip(1).any(|arg| {
        arg == "-r"
            || arg == "--sample-rate"
            || arg.starts_with("-r") && arg.len() > 2 && arg.as_bytes()[2].is_ascii_digit()
            || arg.starts_with("--sample-rate=")
    })
}

#[cfg(target_os = "macos")]
fn extract_output_device_arg(args: &[String]) -> Option<String> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--output-device" {
            return iter.next().cloned();
        }
        if let Some(val) = arg.strip_prefix("--output-device=") {
            return Some(val.to_string());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn query_macos_output_sample_rate(output_device: Option<&str>) -> Option<u32> {
    use std::ffi::{c_char, c_void, CStr};
    use std::mem::size_of;
    use std::ptr::null;

    type AudioObjectID = u32;
    type AudioObjectPropertySelector = u32;
    type AudioObjectPropertyScope = u32;
    type AudioObjectPropertyElement = u32;
    type OSStatus = i32;
    type CFStringRef = *const c_void;
    type CFStringEncoding = u32;

    const K_AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectID = 1;
    const K_AUDIO_HARDWARE_NO_ERROR: OSStatus = 0;
    const K_AUDIO_HARDWARE_PROPERTY_DEVICES: AudioObjectPropertySelector =
        u32::from_be_bytes(*b"dev#");
    const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: AudioObjectPropertySelector =
        u32::from_be_bytes(*b"dOut");
    const K_AUDIO_DEVICE_PROPERTY_DEVICE_NAME_CFSTRING: AudioObjectPropertySelector =
        u32::from_be_bytes(*b"lnam");
    const K_AUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE: AudioObjectPropertySelector =
        u32::from_be_bytes(*b"nsrt");
    const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: AudioObjectPropertyScope =
        u32::from_be_bytes(*b"glob");
    const K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT: AudioObjectPropertyScope =
        u32::from_be_bytes(*b"outp");
    const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: AudioObjectPropertyElement = 0;
    const K_CF_STRING_ENCODING_UTF8: CFStringEncoding = 0x0800_0100;

    #[repr(C)]
    struct AudioObjectPropertyAddress {
        m_selector: AudioObjectPropertySelector,
        m_scope: AudioObjectPropertyScope,
        m_element: AudioObjectPropertyElement,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyDataSize(
            in_object_id: AudioObjectID,
            in_address: *const AudioObjectPropertyAddress,
            in_qualifier_data_size: u32,
            in_qualifier_data: *const c_void,
            out_data_size: *mut u32,
        ) -> OSStatus;

        fn AudioObjectGetPropertyData(
            in_object_id: AudioObjectID,
            in_address: *const AudioObjectPropertyAddress,
            in_qualifier_data_size: u32,
            in_qualifier_data: *const c_void,
            io_data_size: *mut u32,
            out_data: *mut c_void,
        ) -> OSStatus;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringGetCString(
            the_string: CFStringRef,
            buffer: *mut c_char,
            buffer_size: isize,
            encoding: CFStringEncoding,
        ) -> u8;
        fn CFRelease(cf: *const c_void);
    }

    unsafe fn device_name(device_id: AudioObjectID) -> Option<String> {
        let addr = AudioObjectPropertyAddress {
            m_selector: K_AUDIO_DEVICE_PROPERTY_DEVICE_NAME_CFSTRING,
            m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
            m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut cf_str: CFStringRef = null();
        let mut size = size_of::<CFStringRef>() as u32;
        let status = AudioObjectGetPropertyData(
            device_id,
            &addr,
            0,
            null(),
            &mut size,
            (&mut cf_str as *mut CFStringRef).cast(),
        );
        if status != K_AUDIO_HARDWARE_NO_ERROR || cf_str.is_null() {
            return None;
        }
        let mut buf = [0 as c_char; 256];
        let ok = CFStringGetCString(
            cf_str,
            buf.as_mut_ptr(),
            buf.len() as isize,
            K_CF_STRING_ENCODING_UTF8,
        );
        CFRelease(cf_str);
        if ok == 0 {
            return None;
        }
        Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
    }

    unsafe {
        let device_id = match output_device {
            Some(target_name) if !target_name.is_empty() => {
                let addr = AudioObjectPropertyAddress {
                    m_selector: K_AUDIO_HARDWARE_PROPERTY_DEVICES,
                    m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
                    m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
                };
                let mut data_size = 0u32;
                if AudioObjectGetPropertyDataSize(
                    K_AUDIO_OBJECT_SYSTEM_OBJECT,
                    &addr,
                    0,
                    null(),
                    &mut data_size,
                ) != K_AUDIO_HARDWARE_NO_ERROR
                {
                    return None;
                }
                let count = data_size as usize / size_of::<AudioObjectID>();
                let mut devices = vec![0 as AudioObjectID; count];
                if AudioObjectGetPropertyData(
                    K_AUDIO_OBJECT_SYSTEM_OBJECT,
                    &addr,
                    0,
                    null(),
                    &mut data_size,
                    devices.as_mut_ptr().cast(),
                ) != K_AUDIO_HARDWARE_NO_ERROR
                {
                    return None;
                }
                devices
                    .into_iter()
                    .find(|&id| device_name(id).as_deref() == Some(target_name))?
            }
            Some(_) => return None,
            None => {
                let addr = AudioObjectPropertyAddress {
                    m_selector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
                    m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
                    m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
                };
                let mut default_id: AudioObjectID = 0;
                let mut size = size_of::<AudioObjectID>() as u32;
                if AudioObjectGetPropertyData(
                    K_AUDIO_OBJECT_SYSTEM_OBJECT,
                    &addr,
                    0,
                    null(),
                    &mut size,
                    (&mut default_id as *mut AudioObjectID).cast(),
                ) != K_AUDIO_HARDWARE_NO_ERROR
                    || default_id == 0
                {
                    return None;
                }
                default_id
            }
        };

        let rate_addr = AudioObjectPropertyAddress {
            m_selector: K_AUDIO_DEVICE_PROPERTY_NOMINAL_SAMPLE_RATE,
            m_scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            m_element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut rate: f64 = 0.0;
        let mut size = size_of::<f64>() as u32;
        if AudioObjectGetPropertyData(
            device_id,
            &rate_addr,
            0,
            null(),
            &mut size,
            (&mut rate as *mut f64).cast(),
        ) != K_AUDIO_HARDWARE_NO_ERROR
            || rate <= 0.0
        {
            return None;
        }
        Some(rate.round() as u32)
    }
}

fn prepare_standalone_args(mut args: Vec<String>, detected_sample_rate: Option<u32>) -> Vec<String> {
    let is_help_or_version = args
        .iter()
        .skip(1)
        .any(|a| a == "-h" || a == "--help" || a == "-V" || a == "--version");
    if !is_help_or_version && !has_sample_rate_arg(&args) {
        if let Some(sr) = detected_sample_rate {
            args.push(String::from("--sample-rate"));
            args.push(sr.to_string());
        }
    }
    args
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(target_os = "macos")]
    let detected_sr = {
        let out_dev = extract_output_device_arg(&args);
        query_macos_output_sample_rate(out_dev.as_deref())
    };
    #[cfg(not(target_os = "macos"))]
    let detected_sr = None;

    let args = prepare_standalone_args(args, detected_sr);
    nih_export_standalone_with_args::<Plugin303, _>(args);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_detected_sample_rate_when_unspecified() {
        let args = vec!["standalone".to_string()];
        let out = prepare_standalone_args(args, Some(44_100));
        assert_eq!(out, vec!["standalone", "--sample-rate", "44100"]);
    }

    #[test]
    fn preserves_explicit_sample_rate_flags() {
        for flag in [
            vec!["standalone", "-r", "48000"],
            vec!["standalone", "-r48000"],
            vec!["standalone", "--sample-rate", "96000"],
            vec!["standalone", "--sample-rate=96000"],
        ] {
            let input: Vec<String> = flag.into_iter().map(String::from).collect();
            let out = prepare_standalone_args(input.clone(), Some(44_100));
            assert_eq!(out, input);
        }
    }

    #[test]
    fn skips_injection_on_help_or_version() {
        for flag in ["-h", "--help", "-V", "--version"] {
            let input = vec!["standalone".to_string(), flag.to_string()];
            let out = prepare_standalone_args(input.clone(), Some(44_100));
            assert_eq!(out, input);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_queries_default_output_sample_rate() {
        if let Some(sr) = query_macos_output_sample_rate(None) {
            assert!(
                (8_000..=384_000).contains(&sr),
                "unexpected CoreAudio nominal sample rate: {sr}"
            );
        }
    }
}
